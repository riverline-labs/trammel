// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//! Stringify a `syn::Path` as `seg1::seg2::seg3`. Generic args are dropped.

pub fn of(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

#[cfg(test)]
mod tests {
    use super::of;

    fn p(src: &str) -> syn::Path {
        syn::parse_str(src).unwrap()
    }

    #[test]
    fn simple() {
        assert_eq!(of(&p("db")), "db");
        assert_eq!(of(&p("db::User")), "db::User");
        assert_eq!(of(&p("crate::db::User")), "crate::db::User");
    }

    #[test]
    fn drops_generics() {
        assert_eq!(of(&p("Vec<u8>")), "Vec");
        assert_eq!(of(&p("HashMap<String, u32>")), "HashMap");
    }
}
