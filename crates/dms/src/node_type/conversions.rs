use super::DmsNodeType;

impl From<u16> for DmsNodeType {
    fn from(value: u16) -> Self {
        Self(u64::from(value))
    }
}

impl From<u64> for DmsNodeType {
    fn from(value: u64) -> Self {
        Self(value)
    }
}
