use diesel_async::AsyncConnection;
use diesel_async_migrations::{embed_migrations, EmbeddedMigrations};

pub async fn run_migrations<C>(connection: &mut C) -> Result<(), diesel::result::Error>
where
    C: AsyncConnection<Backend = diesel::pg::Pg> + 'static + Send,
{
    let migrations: EmbeddedMigrations = embed_migrations!();

    migrations.run_pending_migrations(connection).await?;

    Ok(())
}
