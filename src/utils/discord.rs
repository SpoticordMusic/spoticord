use serenity::{
  model::{prelude::UserId, user::User},
  prelude::Context,
};

pub fn escape(text: impl Into<String>) -> String {
  let text: String = text.into();

  text
    .replace("\\", "\\\\")
    .replace("*", "\\*")
    .replace("_", "\\_")
    .replace("~", "\\~")
    .replace("`", "\\`")
}

pub async fn get_user(ctx: &Context, id: UserId) -> Option<User> {
  let user = match ctx.cache.user(id) {
    Some(user) => user,
    None => match ctx.http.get_user(id.0).await {
      Ok(user) => user,
      Err(_) => return None,
    },
  };

  Some(user)
}
