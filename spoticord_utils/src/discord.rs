pub enum Colors {
    Info = 0x0773D6,
    Success = 0x3BD65D,
    Warning = 0xF0D932,
    Error = 0xFC1F28,
    None = 0,
}

impl From<Colors> for poise::serenity_prelude::Colour {
    fn from(value: Colors) -> Self {
        Self(value as u32)
    }
}

pub fn escape(text: impl Into<String>) -> String {
    let text: String = text.into();

    text.replace('\\', "\\\\")
        .replace('/', "\\/")
        .replace('*', "\\*")
        .replace('_', "\\_")
        .replace('~', "\\~")
        .replace('`', "\\`")
        // Prevent markdown links
        .replace('[', "\\[")
        .replace(']', "\\]")
}
