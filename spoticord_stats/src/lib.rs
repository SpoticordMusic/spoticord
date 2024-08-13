use redis::{Client, Commands, Connection, RedisResult as Result};

pub struct StatsManager {
    redis: Connection,
}

impl StatsManager {
    pub fn new(url: impl AsRef<str>) -> Result<Self> {
        let client = Client::open(url.as_ref())?;
        let connection = client.get_connection()?;

        Ok(StatsManager { redis: connection })
    }

    pub fn set_active_count(&mut self, count: usize) -> Result<()> {
        self.redis.set("spoticord-active-guilds", count.to_string())
    }
}
