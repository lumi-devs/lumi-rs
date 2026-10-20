use sea_orm_migration::prelude::*;

mod m0001_initial;
mod m0002_hot_indexes;

#[derive(Debug)]
pub struct Migrator;

#[sea_orm_migration::async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m0001_initial::Migration),
            Box::new(m0002_hot_indexes::Migration),
        ]
    }
}
