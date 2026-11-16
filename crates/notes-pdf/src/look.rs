//! The colours of a deck's theme on paper.

/// What a theme paints with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    pub background: u32,
    pub ink: u32,
    /// Headings, links and the bars of quotes.
    pub accent: u32,
    /// Slide numbers and what is set back.
    pub muted: u32,
    /// Behind code and callouts.
    pub tint: u32,
    /// Lines of tables and rules.
    pub rule: u32,
}

/// The look of a theme by its name (the `theme:` of the front matter). The default theme and `light` print light; `dark` is the app's dark scheme.
#[must_use]
pub const fn look(theme: &str) -> Look {
    match theme.as_bytes() {
        b"dark" => Look {
            background: 0x0015_181C,
            ink: 0x00E6_E8EA,
            accent: 0x006E_A8FE,
            muted: 0x009A_A4AE,
            tint: 0x001C_2128,
            rule: 0x002B_3138,
        },
        b"night" => Look {
            background: 0x000F_1220,
            ink: 0x00E8_EBF5,
            accent: 0x008A_B4FF,
            muted: 0x008A_90A8,
            tint: 0x001B_2038,
            rule: 0x0044_4B6B,
        },
        b"paper" => Look {
            background: 0x00FB_F7EE,
            ink: 0x002B_2A27,
            accent: 0x00A4_452C,
            muted: 0x008A_8579,
            tint: 0x00EF_E8D6,
            rule: 0x00CF_C6B0,
        },
        b"contrast" => Look {
            background: 0x0000_0000,
            ink: 0x00FF_FFFF,
            accent: 0x00FF_E14D,
            muted: 0x00C0_C0C0,
            tint: 0x0022_2222,
            rule: 0x00FF_FFFF,
        },
        _ => Look {
            background: 0x00FF_FFFF,
            ink: 0x001F_2328,
            accent: 0x001A_73E8,
            muted: 0x006A_737D,
            tint: 0x00F0_F2F5,
            rule: 0x00D0_D7DE,
        },
    }
}
