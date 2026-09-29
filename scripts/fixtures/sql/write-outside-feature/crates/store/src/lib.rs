pub mod audit;
pub mod greeting;

pub async fn sneak(tx: &mut W) {
    sqlx::query!("delete from audit where id = $1", id);
}
