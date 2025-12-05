#[derive(Default)]
pub enum EmbedColor {
    Info = 0x0773D6,
    Success = 0x3BD65D,
    Warning = 0xF0D932,
    Error = 0xFC1F28,

    #[default]
    None = 0,
}

impl From<EmbedColor> for poise::serenity_prelude::Colour {
    fn from(value: EmbedColor) -> Self {
        Self(value as u32)
    }
}

pub fn escape(text: impl AsRef<str>) -> String {
    let text = text.as_ref();

    text.replace("\\", "\\\\")
        .replace('/', "\\/")
        .replace('*', "\\*")
        .replace('_', "\\_")
        .replace('~', "\\~")
        .replace('`', "\\`")
        // Prevent markdown links
        .replace('[', "\\[")
        .replace(']', "\\]")
        // Prevent small text
        .replace("-#", "\\-#")
}
