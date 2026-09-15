//! Regression for RUSTSEC-2024-0429. Run with --release on Linux.
#[cfg(all(test, target_os = "linux"))]
mod tests {
    use glib::variant::ToVariant;

    #[test]
    fn variant_str_iter_all_affected_operations() {
        // Runtime-owned input also covers empty and multibyte strings.
        let values = ["", "alpha", "café", "日本語", "omega"].map(String::from);
        let variant = values.to_vec().to_variant();
        let expected: Vec<&str> = values.iter().map(String::as_str).collect();
        assert_eq!(
            variant.array_iter_str().unwrap().collect::<Vec<_>>(),
            expected
        );
        assert_eq!(
            variant.array_iter_str().unwrap().rev().collect::<Vec<_>>(),
            expected.iter().rev().copied().collect::<Vec<_>>()
        );
        assert_eq!(variant.array_iter_str().unwrap().last(), Some("omega"));
        let mut iter = variant.array_iter_str().unwrap();
        assert_eq!(iter.next(), Some(""));
        assert_eq!(iter.next_back(), Some("omega"));
        assert_eq!(iter.nth(1), Some("café"));
        assert_eq!(iter.nth_back(0), Some("日本語"));
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next_back(), None);
    }

    #[test]
    fn variant_str_iter_empty_and_out_of_bounds() {
        let empty = Vec::<String>::new().to_variant();
        assert_eq!(empty.array_iter_str().unwrap().next(), None);
        assert_eq!(empty.array_iter_str().unwrap().next_back(), None);
        assert_eq!(empty.array_iter_str().unwrap().last(), None);
        let one = vec![String::from("one")].to_variant();
        assert_eq!(one.array_iter_str().unwrap().nth(usize::MAX), None);
        assert_eq!(one.array_iter_str().unwrap().nth_back(usize::MAX), None);
    }
}
