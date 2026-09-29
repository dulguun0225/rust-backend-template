pub async fn note(tx: &mut W) {
    sqlx::query!("insert into audit (id, note) values ($1, $2)", id, note);
}
pub async fn read_greetings(tx: &mut R) {
    sqlx::query!(r#"select g.id, g.name from greeting g"#);
}
