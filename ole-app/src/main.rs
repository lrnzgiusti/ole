//! OLE - Open Live Engine
//!
//! Cyberpunk DJ application with GPU-accelerated GUI.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::Receiver;

use ole_audio::{AudioCommand, AudioEngine, AudioEvent, EngineState};
use ole_gui::OleApp;

fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .init();

    // Create audio channels
    let (cmd_tx, cmd_rx, evt_tx, evt_rx) = AudioEngine::create_channels();

    // Shutdown flag
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_audio = shutdown.clone();

    // Spawn audio thread (identical to TUI version)
    let audio_handle = thread::spawn(move || {
        run_audio_thread(cmd_rx, evt_tx, shutdown_audio);
    });

    // Configure eframe window
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("OLE - Open Live Engine")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_decorations(true),
        ..Default::default()
    };

    // Run GUI
    let result = eframe::run_native(
        "OLE",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(OleApp::new(cmd_tx, evt_rx)))
        }),
    );

    // Cleanup
    shutdown.store(true, Ordering::SeqCst);
    let _ = audio_handle.join();

    result.map_err(|e| anyhow::anyhow!("eframe error: {}", e))
}

fn run_audio_thread(
    cmd_rx: Receiver<AudioCommand>,
    evt_tx: crossbeam_channel::Sender<AudioEvent>,
    shutdown: Arc<AtomicBool>,
) {
    // Get audio host and device
    let host = cpal::default_host();
    let device = match host.default_output_device() {
        Some(d) => d,
        None => {
            let _ = evt_tx.send(AudioEvent::Error("No audio output device found".into()));
            return;
        }
    };

    let config = match device.default_output_config() {
        Ok(c) => c,
        Err(e) => {
            let _ = evt_tx.send(AudioEvent::Error(format!(
                "Failed to get audio config: {}",
                e
            )));
            return;
        }
    };

    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as usize;

    // Create engine state
    let engine_state = Arc::new(std::sync::Mutex::new(EngineState::new(sample_rate)));
    let engine_for_callback = engine_state.clone();

    // Pre-allocate mono conversion buffer (avoid allocation in audio callback)
    // Size for up to 32768 mono samples -> 65536 stereo samples
    let mut mono_conversion_buffer = vec![0.0f32; 65536];

    // State update interval
    let mut last_state_update = Instant::now();
    let state_update_interval = Duration::from_millis(33); // ~30fps

    // Build audio stream
    let stream = device.build_output_stream(
        &config.into(),
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            if let Ok(mut state) = engine_for_callback.try_lock() {
                if channels == 2 {
                    state.process(data);
                } else if channels == 1 {
                    let stereo_len = data.len() * 2;
                    if stereo_len <= mono_conversion_buffer.len() {
                        let stereo = &mut mono_conversion_buffer[..stereo_len];
                        stereo.fill(0.0);
                        state.process(stereo);
                        for (i, sample) in data.iter_mut().enumerate() {
                            *sample = (stereo[i * 2] + stereo[i * 2 + 1]) * 0.5;
                        }
                    } else {
                        data.fill(0.0);
                    }
                } else {
                    // Multi-channel (>2): process stereo, copy to first 2 channels, silence rest
                    let frames = data.len() / channels;
                    let stereo_len = frames * 2;
                    if stereo_len <= mono_conversion_buffer.len() {
                        let stereo = &mut mono_conversion_buffer[..stereo_len];
                        stereo.fill(0.0);
                        state.process(stereo);
                        for f in 0..frames {
                            data[f * channels] = stereo[f * 2];
                            data[f * channels + 1] = stereo[f * 2 + 1];
                            for ch in 2..channels {
                                data[f * channels + ch] = 0.0;
                            }
                        }
                    } else {
                        data.fill(0.0);
                    }
                }
            } else {
                data.fill(0.0);
            }
        },
        |err| {
            eprintln!("Audio stream error: {}", err);
        },
        None,
    );

    let stream = match stream {
        Ok(s) => s,
        Err(e) => {
            let _ = evt_tx.send(AudioEvent::Error(format!(
                "Failed to create audio stream: {}",
                e
            )));
            return;
        }
    };

    if let Err(e) = stream.play() {
        let _ = evt_tx.send(AudioEvent::Error(format!("Failed to start audio: {}", e)));
        return;
    }

    // Command processing loop
    while !shutdown.load(Ordering::Acquire) {
        match cmd_rx.recv_timeout(Duration::from_millis(10)) {
            Ok(AudioCommand::Shutdown) => break,
            Ok(cmd) => {
                if let Ok(mut state) = engine_state.lock() {
                    state.handle_command(cmd);
                }
            }
            Err(_) => {}
        }

        // Send state updates periodically
        if last_state_update.elapsed() >= state_update_interval {
            if let Ok(state) = engine_state.lock() {
                let _ = evt_tx.try_send(state.get_state());
            }
            last_state_update = Instant::now();
        }
    }
}
