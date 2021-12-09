pub mod de;
mod index;
mod map;
mod merge;
pub mod ser;
mod value;

pub use self::{index::Index, merge::*, ser::to_value, value::*};

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test() {
        let v = Value::String("Test".to_string());

        let out = &v["key"][""];
    }
}
