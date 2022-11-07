use redis::{Commands, RedisResult};

#[derive(Clone)]
pub struct StatsManager {
  redis: redis::Client,
}

impl StatsManager {
  pub fn new(url: impl Into<String>) -> RedisResult<StatsManager> {
    let redis = redis::Client::open(url.into())?;

    Ok(StatsManager { redis })
  }

  pub fn set_server_count(&self, count: usize) -> RedisResult<()> {
    let mut con = self.redis.get_connection()?;

    con.set("sc-bot-total-servers", count.to_string())
  }

  pub fn set_active_count(&self, count: usize) -> RedisResult<()> {
    let mut con = self.redis.get_connection()?;

    con.set("sc-bot-active-servers", count.to_string())
  }
}
