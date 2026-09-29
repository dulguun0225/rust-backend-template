pub async fn sneak(tx: &mut W) {
    sqlx::query!(r#"insert into "audit" (id, note) values ($1, $2)"#, id, note);
}
