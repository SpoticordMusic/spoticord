pub fn escape(text: impl Into<String>) -> String {
  let text: String = text.into();

  text
    .replace('\\', "\\\\")
    .replace('/', "\\/")
    .replace('*', "\\*")
    .replace('_', "\\_")
    .replace('~', "\\~")
    .replace('`', "\\`")
    .replace('[', "\\[")
    .replace(']', "\\]")
}
