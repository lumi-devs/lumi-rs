use sea_orm_migration::prelude::*;

mod m0001_initial;
mod m0002_schema_indexes;
mod m0003_drop_guild_case_counter;
mod m0004_drop_guild_config;

#[derive(Debug)]
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m0001_initial::Migration),
            Box::new(m0002_schema_indexes::Migration),
            Box::new(m0003_drop_guild_case_counter::Migration),
            Box::new(m0004_drop_guild_config::Migration),
        ]
    }
}
