pub enum Color {
    Info = 0x0773D6,
    Success = 0x3BD65D,
    Warning = 0xF0D932,
    Error = 0xFC1F28,
    None = 0,
}

impl From<Color> for poise::serenity_prelude::utils::Colour {
    fn from(value: Color) -> Self {
        Self(value as u32)
    }
}
