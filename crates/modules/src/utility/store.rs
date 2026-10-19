use lumi_kernel::cache::Cache;

/// Atomically claim a cooldown slot. Returns `true` when the caller is allowed
/// to proceed, `false` when the key is still held.
pub async fn claim_cooldown(cache: &Cache, key: &str, ttl_ms: u64) -> Result<bool, redis::RedisError> {
    let mut conn = cache.redis();
    let set: Option<String> = redis::cmd("SET")
        .arg(key)
        .arg("1")
        .arg("NX")
        .arg("PX")
        .arg(ttl_ms)
        .query_async(&mut conn)
        .await?;
    Ok(set.is_some())
}

/// Remaining time on a cooldown key in milliseconds (`0` when unset/expired).
pub async fn cooldown_remaining_ms(cache: &Cache, key: &str) -> Result<i64, redis::RedisError> {
    let mut conn = cache.redis();
    let ttl: i64 = redis::cmd("PTTL").arg(key).query_async(&mut conn).await?;
    Ok(ttl.max(0))
}
