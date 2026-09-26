//! 通讯平台 → 连接器平台（iMessage 等）：现在连着哪些连接器。
//!
//! 配置本身走设置页的配置草稿（`platforms.connectors.<平台>`），这里只报连接状态。

use crate::web::*;

pub(in crate::web) async fn connectors_status(
    State(state): State<DaemonState>,
    headers: HeaderMap,
) -> std::result::Result<Json<Value>, ApiError> {
    require_admin(&headers, &state)?;
    let connected: Vec<Value> = state
        .platforms
        .connectors
        .connected()
        .into_iter()
        .map(|handle| {
            let connected_at = handle
                .connected_at
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_secs())
                .unwrap_or_default();
            json!({
                "platform": handle.platform,
                "account": handle.account,
                "display_name": handle.display_name,
                "connector": handle.connector_name,
                "version": handle.connector_version,
                "capabilities": handle.capabilities,
                "connected_at": connected_at,
            })
        })
        .collect();
    Ok(Json(json!({ "connected": connected })))
}
