#[cfg(test)]
mod test_minimal {
    #[test]
    fn test_naga_minimal() {
        let src = "fn is_black_key(key: u32) -> bool { let note = key % 12u; return note == 1u || note == 3u || note == 6u || note == 8u || note == 10u; }";
        let module = naga::front::wgsl::parse_str(src).unwrap_or_else(|e| {
            panic!("parse error: {e:?}");
        });
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| {
            panic!("validation error: {e:?}");
        });
    }
}
