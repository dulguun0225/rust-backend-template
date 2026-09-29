pub async fn insert(tx: &mut W) {
    sqlx::query!("insert into greeting (id, name, created_at, version) values ($1, $2, $3, 1)", id, name, at);
}
pub async fn rename(tx: &mut W) {
    sqlx::query!(
        "update greeting set name = $3, version = version + 1 where id = $1 and version = $2",
        id, v, name
    );
    sqlx::query_scalar!("select version from greeting where id = $1", id);
}
pub async fn page(tx: &mut R) {
    sqlx::query_as!(Row, "select id from greeting where (created_at, id) < ($1, $2) order by created_at desc, id desc limit $3", a, b, c);
}
pub async fn locked(tx: &mut W) {
    sqlx::query!("select id from greeting where id = $1 for update", id);
}
pub async fn mint(tx: &mut W) {
    sqlx::query!("insert into greeting (id, name, created_at, version) values (gen_random_uuid(), $1, $2, 1)", name, at);
}
