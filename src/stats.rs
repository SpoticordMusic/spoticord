use redis::{Client, Commands, RedisResult as Result};

#[derive(Clone)]
pub struct StatsManager {
  redis: Client,
}

impl StatsManager {
  pub fn new(url: impl AsRef<str>) -> Result<Self> {
    let redis = Client::open(url.as_ref())?;

    Ok(StatsManager { redis })
  }

  pub fn set_active_count(&self, count: usize) -> Result<()> {
    let mut con = self.redis.get_connection()?;

    con.set("sc-bot-active-servers", count.to_string())
  }
}
