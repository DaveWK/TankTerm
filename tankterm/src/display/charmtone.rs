//! Charm's CharmTone color palette, so colors can be configured by name like `"charple"`.
//!
//! The names and values are from `charmbracelet/x/exp/charmtone`.

use crate::display::color::Rgb;

/// Every CharmTone color, by lowercase name.
const COLORS: &[(&str, Rgb)] = &[
    // Spectrum.
    ("cumin", Rgb::new(0xbf, 0x97, 0x6f)),
    ("tang", Rgb::new(0xff, 0x98, 0x5a)),
    ("yam", Rgb::new(0xff, 0xb5, 0x87)),
    ("paprika", Rgb::new(0xd3, 0x6c, 0x64)),
    ("bengal", Rgb::new(0xff, 0x6e, 0x63)),
    ("uni", Rgb::new(0xff, 0x93, 0x7d)),
    ("sriracha", Rgb::new(0xeb, 0x42, 0x68)),
    ("coral", Rgb::new(0xff, 0x57, 0x7d)),
    ("salmon", Rgb::new(0xff, 0x7f, 0x90)),
    ("chili", Rgb::new(0xe2, 0x30, 0x80)),
    ("cherry", Rgb::new(0xff, 0x38, 0x8b)),
    ("tuna", Rgb::new(0xff, 0x6d, 0xaa)),
    ("macaron", Rgb::new(0xe9, 0x40, 0xb0)),
    ("pony", Rgb::new(0xff, 0x4f, 0xbf)),
    ("cheeky", Rgb::new(0xff, 0x79, 0xd0)),
    ("flamingo", Rgb::new(0xf9, 0x47, 0xe3)),
    ("dolly", Rgb::new(0xff, 0x60, 0xff)),
    ("blush", Rgb::new(0xff, 0x84, 0xff)),
    ("urchin", Rgb::new(0xc3, 0x37, 0xe0)),
    ("mochi", Rgb::new(0xeb, 0x5d, 0xff)),
    ("lilac", Rgb::new(0xf3, 0x79, 0xff)),
    ("prince", Rgb::new(0x9c, 0x35, 0xe1)),
    ("violet", Rgb::new(0xc2, 0x59, 0xff)),
    ("mauve", Rgb::new(0xd4, 0x6e, 0xff)),
    ("grape", Rgb::new(0x71, 0x34, 0xdd)),
    ("plum", Rgb::new(0x99, 0x53, 0xff)),
    ("orchid", Rgb::new(0xad, 0x6e, 0xff)),
    ("jelly", Rgb::new(0x4a, 0x30, 0xd9)),
    ("charple", Rgb::new(0x6b, 0x50, 0xff)),
    ("hazy", Rgb::new(0x8b, 0x75, 0xff)),
    ("ox", Rgb::new(0x33, 0x31, 0xb2)),
    ("sapphire", Rgb::new(0x49, 0x49, 0xff)),
    ("guppy", Rgb::new(0x72, 0x72, 0xff)),
    ("oceania", Rgb::new(0x2b, 0x55, 0xb3)),
    ("thunder", Rgb::new(0x47, 0x76, 0xff)),
    ("anchovy", Rgb::new(0x71, 0x9a, 0xfc)),
    ("damson", Rgb::new(0x00, 0x7a, 0xb8)),
    ("malibu", Rgb::new(0x00, 0xa4, 0xff)),
    ("sardine", Rgb::new(0x4f, 0xbe, 0xfe)),
    ("zinc", Rgb::new(0x10, 0xb1, 0xae)),
    ("turtle", Rgb::new(0x0a, 0xdc, 0xd9)),
    ("lichen", Rgb::new(0x5c, 0xdf, 0xea)),
    ("guac", Rgb::new(0x12, 0xc7, 0x8f)),
    ("julep", Rgb::new(0x00, 0xff, 0xb2)),
    ("bok", Rgb::new(0x68, 0xff, 0xd6)),
    ("mustard", Rgb::new(0xf5, 0xef, 0x34)),
    ("citron", Rgb::new(0xe8, 0xff, 0x27)),
    ("zest", Rgb::new(0xe8, 0xfe, 0x96)),
    ("butter", Rgb::new(0xff, 0xfa, 0xf1)),
    // Neutrals, darkest to lightest.
    ("pepper", Rgb::new(0x20, 0x1f, 0x26)),
    ("bbq", Rgb::new(0x2d, 0x2c, 0x36)),
    ("char", Rgb::new(0x3a, 0x39, 0x43)),
    ("iron", Rgb::new(0x4d, 0x4c, 0x57)),
    ("oyster", Rgb::new(0x60, 0x5f, 0x6b)),
    ("squid", Rgb::new(0x85, 0x83, 0x92)),
    ("steam", Rgb::new(0xa2, 0xa0, 0xad)),
    ("smoke", Rgb::new(0xbf, 0xbc, 0xc8)),
    ("steep", Rgb::new(0xd6, 0xd3, 0xdc)),
    ("sash", Rgb::new(0xec, 0xeb, 0xf0)),
    ("salt", Rgb::new(0xf7, 0xf6, 0xfb)),
    ("soda", Rgb::new(0xfb, 0xfb, 0xfb)),
    // Charple variants.
    ("darple", Rgb::new(0x5b, 0x40, 0xec)),
    ("larple", Rgb::new(0x7b, 0x62, 0xff)),
    // Diff additions and deletions.
    ("pickle", Rgb::new(0x00, 0xa4, 0x75)),
    ("gator", Rgb::new(0x18, 0x46, 0x3d)),
    ("spinach", Rgb::new(0x1c, 0x36, 0x34)),
    ("pom", Rgb::new(0xab, 0x24, 0x54)),
    ("steak", Rgb::new(0x58, 0x22, 0x38)),
    ("toast", Rgb::new(0x41, 0x21, 0x30)),
    // Secondary outside the spectrum.
    ("ice", Rgb::new(0x00, 0xff, 0xfc)),
    // Deprecated aliases.
    ("charcoal", Rgb::new(0x3a, 0x39, 0x43)),
    ("ash", Rgb::new(0xec, 0xeb, 0xf0)),
];

/// Look up a CharmTone color by name, ignoring case.
pub fn color(name: &str) -> Option<Rgb> {
    COLORS.iter().find(|(color, _)| color.eq_ignore_ascii_case(name)).map(|&(_, rgb)| rgb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_lowercase() {
        for (index, (name, _)) in COLORS.iter().enumerate() {
            assert_eq!(*name, name.to_lowercase(), "{name}");
            assert!(COLORS[index + 1..].iter().all(|(other, _)| other != name), "{name}");
        }
    }

    #[test]
    fn primaries() {
        assert_eq!(color("Charple"), Some(Rgb::new(0x6b, 0x50, 0xff)));
        assert_eq!(color("dolly"), Some(Rgb::new(0xff, 0x60, 0xff)));
        assert_eq!(color("JULEP"), Some(Rgb::new(0x00, 0xff, 0xb2)));
        assert_eq!(color("zest"), Some(Rgb::new(0xe8, 0xfe, 0x96)));
        assert_eq!(color("not a color"), None);
    }

    #[test]
    fn aliases_match_their_colors() {
        assert_eq!(color("charcoal"), color("char"));
        assert_eq!(color("ash"), color("sash"));
    }
}
