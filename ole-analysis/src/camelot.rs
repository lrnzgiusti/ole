//! Camelot wheel notation for harmonic mixing
//!
//! Maps musical keys to Camelot notation (1A-12B) and provides
//! compatibility checking for harmonic mixing.

use std::fmt;

/// Musical key (24 possible: 12 major + 12 minor)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MusicalKey {
    CMajor,
    DbMajor,
    DMajor,
    EbMajor,
    EMajor,
    FMajor,
    GbMajor,
    GMajor,
    AbMajor,
    AMajor,
    BbMajor,
    BMajor,
    CMinor,
    DbMinor,
    DMinor,
    EbMinor,
    EMinor,
    FMinor,
    GbMinor,
    GMinor,
    AbMinor,
    AMinor,
    BbMinor,
    BMinor,
}

impl MusicalKey {
    /// Get the pitch class (0-11, where 0=C) for this key's root
    pub fn root_pitch_class(&self) -> u8 {
        use MusicalKey::*;
        match self {
            CMajor | CMinor => 0,
            DbMajor | DbMinor => 1,
            DMajor | DMinor => 2,
            EbMajor | EbMinor => 3,
            EMajor | EMinor => 4,
            FMajor | FMinor => 5,
            GbMajor | GbMinor => 6,
            GMajor | GMinor => 7,
            AbMajor | AbMinor => 8,
            AMajor | AMinor => 9,
            BbMajor | BbMinor => 10,
            BMajor | BMinor => 11,
        }
    }

    /// Check if this key is major
    pub fn is_major(&self) -> bool {
        use MusicalKey::*;
        matches!(
            self,
            CMajor
                | DbMajor
                | DMajor
                | EbMajor
                | EMajor
                | FMajor
                | GbMajor
                | GMajor
                | AbMajor
                | AMajor
                | BbMajor
                | BMajor
        )
    }

    /// Get major key from pitch class (0-11)
    pub fn major_from_pitch_class(pc: u8) -> Self {
        use MusicalKey::*;
        match pc % 12 {
            0 => CMajor,
            1 => DbMajor,
            2 => DMajor,
            3 => EbMajor,
            4 => EMajor,
            5 => FMajor,
            6 => GbMajor,
            7 => GMajor,
            8 => AbMajor,
            9 => AMajor,
            10 => BbMajor,
            11 => BMajor,
            _ => unreachable!(),
        }
    }

    /// Get minor key from pitch class (0-11)
    pub fn minor_from_pitch_class(pc: u8) -> Self {
        use MusicalKey::*;
        match pc % 12 {
            0 => CMinor,
            1 => DbMinor,
            2 => DMinor,
            3 => EbMinor,
            4 => EMinor,
            5 => FMinor,
            6 => GbMinor,
            7 => GMinor,
            8 => AbMinor,
            9 => AMinor,
            10 => BbMinor,
            11 => BMinor,
            _ => unreachable!(),
        }
    }
}

impl fmt::Display for MusicalKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use MusicalKey::*;
        let s = match self {
            CMajor => "C",
            DbMajor => "Db",
            DMajor => "D",
            EbMajor => "Eb",
            EMajor => "E",
            FMajor => "F",
            GbMajor => "Gb",
            GMajor => "G",
            AbMajor => "Ab",
            AMajor => "A",
            BbMajor => "Bb",
            BMajor => "B",
            CMinor => "Cm",
            DbMinor => "Dbm",
            DMinor => "Dm",
            EbMinor => "Ebm",
            EMinor => "Em",
            FMinor => "Fm",
            GbMinor => "Gbm",
            GMinor => "Gm",
            AbMinor => "Abm",
            AMinor => "Am",
            BbMinor => "Bbm",
            BMinor => "Bm",
        };
        write!(f, "{}", s)
    }
}

/// Camelot wheel notation (1A-12B)
///
/// The Camelot wheel is a tool for harmonic mixing that arranges keys
/// in a circle where adjacent keys are harmonically compatible.
/// - Numbers 1-12 represent positions on the wheel
/// - 'A' suffix = minor keys
/// - 'B' suffix = major keys
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CamelotKey {
    /// Position on the wheel (1-12)
    pub number: u8,
    /// true = B (major), false = A (minor)
    pub is_major: bool,
}

impl CamelotKey {
    /// Create a new Camelot key
    pub fn new(number: u8, is_major: bool) -> Option<Self> {
        if (1..=12).contains(&number) {
            Some(Self { number, is_major })
        } else {
            None
        }
    }

    /// Convert from musical key to Camelot notation
    ///
    /// Mapping follows the standard Camelot wheel:
    /// - Circle of fifths arrangement
    /// - Minor keys get 'A', major keys get 'B'
    /// - Relative major/minor share the same number
    pub fn from_musical_key(key: MusicalKey) -> Self {
        use MusicalKey::*;
        match key {
            // Minor keys (A) - arranged by circle of fifths
            AbMinor => CamelotKey {
                number: 1,
                is_major: false,
            },
            EbMinor => CamelotKey {
                number: 2,
                is_major: false,
            },
            BbMinor => CamelotKey {
                number: 3,
                is_major: false,
            },
            FMinor => CamelotKey {
                number: 4,
                is_major: false,
            },
            CMinor => CamelotKey {
                number: 5,
                is_major: false,
            },
            GMinor => CamelotKey {
                number: 6,
                is_major: false,
            },
            DMinor => CamelotKey {
                number: 7,
                is_major: false,
            },
            AMinor => CamelotKey {
                number: 8,
                is_major: false,
            },
            EMinor => CamelotKey {
                number: 9,
                is_major: false,
            },
            BMinor => CamelotKey {
                number: 10,
                is_major: false,
            },
            GbMinor => CamelotKey {
                number: 11,
                is_major: false,
            },
            DbMinor => CamelotKey {
                number: 12,
                is_major: false,
            },

            // Major keys (B) - relative majors share the same number
            BMajor => CamelotKey {
                number: 1,
                is_major: true,
            },
            GbMajor => CamelotKey {
                number: 2,
                is_major: true,
            },
            DbMajor => CamelotKey {
                number: 3,
                is_major: true,
            },
            AbMajor => CamelotKey {
                number: 4,
                is_major: true,
            },
            EbMajor => CamelotKey {
                number: 5,
                is_major: true,
            },
            BbMajor => CamelotKey {
                number: 6,
                is_major: true,
            },
            FMajor => CamelotKey {
                number: 7,
                is_major: true,
            },
            CMajor => CamelotKey {
                number: 8,
                is_major: true,
            },
            GMajor => CamelotKey {
                number: 9,
                is_major: true,
            },
            DMajor => CamelotKey {
                number: 10,
                is_major: true,
            },
            AMajor => CamelotKey {
                number: 11,
                is_major: true,
            },
            EMajor => CamelotKey {
                number: 12,
                is_major: true,
            },
        }
    }

    /// Convert to musical key
    pub fn to_musical_key(&self) -> MusicalKey {
        if self.is_major {
            match self.number {
                1 => MusicalKey::BMajor,
                2 => MusicalKey::GbMajor,
                3 => MusicalKey::DbMajor,
                4 => MusicalKey::AbMajor,
                5 => MusicalKey::EbMajor,
                6 => MusicalKey::BbMajor,
                7 => MusicalKey::FMajor,
                8 => MusicalKey::CMajor,
                9 => MusicalKey::GMajor,
                10 => MusicalKey::DMajor,
                11 => MusicalKey::AMajor,
                12 => MusicalKey::EMajor,
                _ => MusicalKey::CMajor, // Fallback
            }
        } else {
            match self.number {
                1 => MusicalKey::AbMinor,
                2 => MusicalKey::EbMinor,
                3 => MusicalKey::BbMinor,
                4 => MusicalKey::FMinor,
                5 => MusicalKey::CMinor,
                6 => MusicalKey::GMinor,
                7 => MusicalKey::DMinor,
                8 => MusicalKey::AMinor,
                9 => MusicalKey::EMinor,
                10 => MusicalKey::BMinor,
                11 => MusicalKey::GbMinor,
                12 => MusicalKey::DbMinor,
                _ => MusicalKey::AMinor, // Fallback
            }
        }
    }

    /// Get display string (e.g., "8A", "12B")
    pub fn display(&self) -> String {
        format!("{}{}", self.number, if self.is_major { 'B' } else { 'A' })
    }

    /// Parse from string (e.g., "8A", "12B")
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.len() < 2 {
            return None;
        }

        let last = s.chars().last()?;
        let is_major = match last.to_ascii_uppercase() {
            'B' => true,
            'A' => false,
            _ => return None,
        };

        let num_part = &s[..s.len() - 1];
        let number: u8 = num_part.parse().ok()?;

        Self::new(number, is_major)
    }

    /// Check if two keys are harmonically compatible for mixing
    ///
    /// Compatible combinations:
    /// 1. Same key (e.g., 8A ↔ 8A)
    /// 2. Adjacent on wheel, same letter (e.g., 8A ↔ 7A, 8A ↔ 9A)
    /// 3. Same number, different letter (relative major/minor, e.g., 8A ↔ 8B)
    pub fn is_compatible(&self, other: &CamelotKey) -> bool {
        // Same key
        if *self == *other {
            return true;
        }

        // Same number, different letter (relative major/minor)
        if self.number == other.number {
            return true;
        }

        // Adjacent on wheel (±1), same letter
        // Wheel wraps: 12 + 1 = 1, 1 - 1 = 12
        if self.is_major == other.is_major {
            let diff = (self.number as i8 - other.number as i8).abs();
            if diff == 1 || diff == 11 {
                return true;
            }
        }

        false
    }

    /// Get the wheel distance between two keys
    ///
    /// Returns a value indicating how "far" two keys are on the Camelot wheel.
    /// Lower values = more compatible.
    /// - 0: Same key
    /// - 1: Adjacent (±1) or relative major/minor
    /// - 2+: Less compatible
    pub fn wheel_distance(&self, other: &CamelotKey) -> u8 {
        if *self == *other {
            return 0;
        }

        // Calculate number distance on the wheel (shortest path)
        let num_diff = {
            let d = (self.number as i8 - other.number as i8).abs();
            d.min(12 - d) as u8
        };

        // Mode difference (major vs minor)
        let mode_diff = if self.is_major != other.is_major {
            1
        } else {
            0
        };

        // Combine distances
        // Same number, different mode = 1
        // Adjacent number, same mode = 1
        // Otherwise add them
        if num_diff == 0 {
            mode_diff
        } else if mode_diff == 0 {
            num_diff
        } else {
            num_diff + mode_diff
        }
    }

    /// Get all compatible keys
    pub fn compatible_keys(&self) -> Vec<CamelotKey> {
        let mut keys = vec![*self];

        // Relative major/minor (same number, different letter)
        keys.push(CamelotKey {
            number: self.number,
            is_major: !self.is_major,
        });

        // Adjacent on wheel (same letter)
        let prev = if self.number == 1 {
            12
        } else {
            self.number - 1
        };
        let next = if self.number == 12 {
            1
        } else {
            self.number + 1
        };

        keys.push(CamelotKey {
            number: prev,
            is_major: self.is_major,
        });
        keys.push(CamelotKey {
            number: next,
            is_major: self.is_major,
        });

        keys
    }
}

impl fmt::Display for CamelotKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camelot_from_musical_key() {
        // Test some common keys
        assert_eq!(
            CamelotKey::from_musical_key(MusicalKey::CMajor),
            CamelotKey {
                number: 8,
                is_major: true
            }
        );
        assert_eq!(
            CamelotKey::from_musical_key(MusicalKey::AMinor),
            CamelotKey {
                number: 8,
                is_major: false
            }
        );
        assert_eq!(
            CamelotKey::from_musical_key(MusicalKey::GMajor),
            CamelotKey {
                number: 9,
                is_major: true
            }
        );
        assert_eq!(
            CamelotKey::from_musical_key(MusicalKey::EMinor),
            CamelotKey {
                number: 9,
                is_major: false
            }
        );
    }

    #[test]
    fn test_camelot_display() {
        assert_eq!(
            CamelotKey {
                number: 8,
                is_major: true
            }
            .display(),
            "8B"
        );
        assert_eq!(
            CamelotKey {
                number: 12,
                is_major: false
            }
            .display(),
            "12A"
        );
    }

    #[test]
    fn test_camelot_parse() {
        assert_eq!(
            CamelotKey::parse("8B"),
            Some(CamelotKey {
                number: 8,
                is_major: true
            })
        );
        assert_eq!(
            CamelotKey::parse("12A"),
            Some(CamelotKey {
                number: 12,
                is_major: false
            })
        );
        assert_eq!(CamelotKey::parse("13A"), None);
        assert_eq!(CamelotKey::parse("0B"), None);
        assert_eq!(CamelotKey::parse("invalid"), None);
    }

    #[test]
    fn test_compatibility_same_key() {
        let key = CamelotKey {
            number: 8,
            is_major: false,
        };
        assert!(key.is_compatible(&key));
    }

    #[test]
    fn test_compatibility_relative_major_minor() {
        // 8A (Am) and 8B (C) are relative major/minor
        let minor = CamelotKey {
            number: 8,
            is_major: false,
        };
        let major = CamelotKey {
            number: 8,
            is_major: true,
        };
        assert!(minor.is_compatible(&major));
        assert!(major.is_compatible(&minor));
    }

    #[test]
    fn test_compatibility_adjacent() {
        let key = CamelotKey {
            number: 8,
            is_major: false,
        };
        let prev = CamelotKey {
            number: 7,
            is_major: false,
        };
        let next = CamelotKey {
            number: 9,
            is_major: false,
        };

        assert!(key.is_compatible(&prev));
        assert!(key.is_compatible(&next));
    }

    #[test]
    fn test_compatibility_wrap_around() {
        // 1A should be compatible with 12A
        let one = CamelotKey {
            number: 1,
            is_major: false,
        };
        let twelve = CamelotKey {
            number: 12,
            is_major: false,
        };
        assert!(one.is_compatible(&twelve));
        assert!(twelve.is_compatible(&one));
    }

    #[test]
    fn test_incompatible_keys() {
        // 8A and 3A are not compatible (too far apart)
        let a = CamelotKey {
            number: 8,
            is_major: false,
        };
        let b = CamelotKey {
            number: 3,
            is_major: false,
        };
        assert!(!a.is_compatible(&b));

        // 8A and 3B are not compatible
        let c = CamelotKey {
            number: 3,
            is_major: true,
        };
        assert!(!a.is_compatible(&c));
    }

    #[test]
    fn test_wheel_distance() {
        let key = CamelotKey {
            number: 8,
            is_major: false,
        };

        // Same key = 0
        assert_eq!(key.wheel_distance(&key), 0);

        // Relative major/minor = 1
        let relative = CamelotKey {
            number: 8,
            is_major: true,
        };
        assert_eq!(key.wheel_distance(&relative), 1);

        // Adjacent same mode = 1
        let adjacent = CamelotKey {
            number: 9,
            is_major: false,
        };
        assert_eq!(key.wheel_distance(&adjacent), 1);

        // Far away = higher
        let far = CamelotKey {
            number: 3,
            is_major: false,
        };
        assert!(key.wheel_distance(&far) > 2);
    }

    #[test]
    fn test_compatible_keys_list() {
        let key = CamelotKey {
            number: 8,
            is_major: false,
        };
        let compatible = key.compatible_keys();

        assert!(compatible.contains(&key)); // Self
        assert!(compatible.contains(&CamelotKey {
            number: 8,
            is_major: true
        })); // Relative
        assert!(compatible.contains(&CamelotKey {
            number: 7,
            is_major: false
        })); // Prev
        assert!(compatible.contains(&CamelotKey {
            number: 9,
            is_major: false
        })); // Next
    }

    #[test]
    fn test_roundtrip_musical_key() {
        // Test that converting to Camelot and back preserves the key
        for key in [
            MusicalKey::CMajor,
            MusicalKey::AMinor,
            MusicalKey::GMajor,
            MusicalKey::EMinor,
            MusicalKey::DbMajor,
            MusicalKey::BbMinor,
        ] {
            let camelot = CamelotKey::from_musical_key(key);
            let back = camelot.to_musical_key();
            assert_eq!(key, back);
        }
    }
}
