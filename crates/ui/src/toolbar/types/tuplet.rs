//! 连音和符点类型定义

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TupletType {
    #[default]
    None,
    Triplet,
    Quintuplet,
    Sextuplet,
    Septuplet,
}

impl std::fmt::Display for TupletType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            TupletType::None => "（无）",
            TupletType::Triplet => "3",
            TupletType::Quintuplet => "5",
            TupletType::Sextuplet => "6",
            TupletType::Septuplet => "7",
        };
        write!(f, "{}", name)
    }
}

impl TupletType {
    pub fn all() -> &'static [TupletType] {
        &[
            TupletType::None,
            TupletType::Triplet,
            TupletType::Quintuplet,
            TupletType::Sextuplet,
            TupletType::Septuplet,
        ]
    }

    pub fn value(&self) -> u32 {
        match self {
            TupletType::None => 1,
            TupletType::Triplet => 3,
            TupletType::Quintuplet => 5,
            TupletType::Sextuplet => 6,
            TupletType::Septuplet => 7,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DotType {
    #[default]
    None,
    Tuplet,
    Single,
    Double,
}

impl std::fmt::Display for DotType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            DotType::None => "（无）",
            DotType::Tuplet => "连音符",
            DotType::Single => "符点",
            DotType::Double => "双符点",
        };
        write!(f, "{}", name)
    }
}

impl DotType {
    pub fn all() -> &'static [DotType] {
        &[
            DotType::None,
            DotType::Tuplet,
            DotType::Single,
            DotType::Double,
        ]
    }

    pub fn multiplier(&self) -> f32 {
        match self {
            DotType::None => 1.0,
            DotType::Tuplet => 1.0,
            DotType::Single => 1.5,
            DotType::Double => 1.75,
        }
    }
}
