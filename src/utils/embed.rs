use serenity::builder::CreateEmbed;

#[allow(dead_code)]
pub enum Status {
  Info = 0x0773D6,
  Success = 0x3BD65D,
  Warning = 0xF0D932,
  Error = 0xFC1F28,
  None = 0,
}

#[derive(Default)]
pub struct EmbedMessageOptions {
  pub title: Option<String>,
  pub title_url: Option<String>,
  pub icon_url: Option<String>,
  pub description: String,
  pub status: Option<Status>,
  pub footer: Option<String>,
}

pub struct EmbedBuilder {
  embed: EmbedMessageOptions,
}

impl EmbedBuilder {
  pub fn new() -> Self {
    Self {
      embed: EmbedMessageOptions::default(),
    }
  }

  pub fn title(mut self, title: impl Into<String>) -> Self {
    self.embed.title = Some(title.into());
    self
  }

  pub fn title_url(mut self, title_url: impl Into<String>) -> Self {
    self.embed.title_url = Some(title_url.into());
    self
  }

  pub fn icon_url(mut self, icon_url: impl Into<String>) -> Self {
    self.embed.icon_url = Some(icon_url.into());
    self
  }

  pub fn description(mut self, description: impl Into<String>) -> Self {
    self.embed.description = description.into();
    self
  }

  pub fn status(mut self, status: Status) -> Self {
    self.embed.status = Some(status);
    self
  }

  pub fn footer(mut self, footer: impl Into<String>) -> Self {
    self.embed.footer = Some(footer.into());
    self
  }

  /// Build the embed
  pub fn build(self) -> EmbedMessageOptions {
    self.embed
  }
}

pub fn make_embed_message<'a>(
  embed: &'a mut CreateEmbed,
  options: EmbedMessageOptions,
) -> &'a mut CreateEmbed {
  let status = options.status.unwrap_or(Status::None);

  embed.author(|author| {
    if let Some(title) = options.title {
      author.name(title);
    }

    if let Some(title_url) = options.title_url {
      author.url(title_url);
    }

    if let Some(icon_url) = options.icon_url {
      author.icon_url(icon_url);
    }

    author
  });

  if let Some(text) = options.footer {
    embed.footer(|footer| footer.text(text));
  }

  embed.description(options.description);
  embed.color(status as u32);

  embed
}
