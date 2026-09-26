//! 普通模式首页的「今天」:登录者所在城市的天气(09-26)。
//!
//! 城市按账号存(和头像同一个目录的 account-home.json),由用户在首页卡片上
//! 自己填——不用浏览器定位,不弹权限、不上传精确位置。天气走 Open-Meteo
//! (内置 get_weather 脚本同一个数据源,免 key),daemon 代取:页面 CSP 只许
//! 连自己。同一城市 15 分钟内复用上一次结果。
//!
//! 这里只给原始数字与 WMO 天气码;「带伞/穿搭/防晒」这些建议是前端按规则
//! 算的(web/features/today.js),改文案不用重编译。

use crate::web::*;
use std::time::{Duration, Instant};

const HOME_FILE: &str = "account-home.json";
const MAX_CITY_CHARS: usize = 40;
const CACHE_TTL: Duration = Duration::from_secs(15 * 60);
const FETCH_TIMEOUT: Duration = Duration::from_secs(8);
const GEOCODING_URL: &str = "https://geocoding-api.open-meteo.com/v1/search";
const FORECAST_URL: &str = "https://api.open-meteo.com/v1/forecast";

static WEATHER_CACHE: Mutex<Option<HashMap<String, (Instant, Value)>>> = Mutex::new(None);

#[derive(Default, Deserialize, Serialize)]
struct HomeSettings {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    city: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::web) struct TodayRequest {
    city: String,
}

fn load_home(dir: &FilePath) -> HomeSettings {
    std::fs::read_to_string(dir.join(HOME_FILE))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_home(dir: &FilePath, settings: &HomeSettings) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = dir.join(HOME_FILE);
    std::fs::write(&path, serde_json::to_string_pretty(settings)?)
        .with_context(|| format!("writing {}", path.display()))
}

/// 城市名收一收:去首尾空白、压掉控制字符、限长。空串 = 清除。
fn normalize_city(raw: &str) -> String {
    raw.chars()
        .filter(|ch| !ch.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(MAX_CITY_CHARS)
        .collect()
}

fn cached(city: &str) -> Option<Value> {
    let guard = WEATHER_CACHE.lock().unwrap();
    let (at, value) = guard.as_ref()?.get(city)?;
    (at.elapsed() < CACHE_TTL).then(|| value.clone())
}

fn remember(city: &str, value: &Value) {
    let mut guard = WEATHER_CACHE.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    // 账号不多,城市更少;防御性地别让它无限长。
    if map.len() > 64 {
        map.clear();
    }
    map.insert(city.to_string(), (Instant::now(), value.clone()));
}

async fn get_json(url: &str, query: &[(&str, String)]) -> anyhow::Result<Value> {
    let response = crate::tools::shared_http_client()
        .get(url)
        .query(query)
        .timeout(FETCH_TIMEOUT)
        .send()
        .await?
        .error_for_status()?;
    Ok(response.json().await?)
}

/// 城市 → 坐标 → 当前天气与今日最高最低。失败返回 None(前端显示「暂时查不到」)。
async fn fetch_weather(city: &str) -> anyhow::Result<Option<Value>> {
    let geo = get_json(
        GEOCODING_URL,
        &[
            ("name", city.to_string()),
            ("count", "1".to_string()),
            ("language", "zh".to_string()),
            ("format", "json".to_string()),
        ],
    )
    .await?;
    let Some(place) = geo["results"].get(0) else {
        return Ok(None);
    };
    let (Some(latitude), Some(longitude)) =
        (place["latitude"].as_f64(), place["longitude"].as_f64())
    else {
        return Ok(None);
    };
    let forecast = get_json(
        FORECAST_URL,
        &[
            ("latitude", latitude.to_string()),
            ("longitude", longitude.to_string()),
            (
                "current",
                "temperature_2m,apparent_temperature,relative_humidity_2m,weather_code,wind_speed_10m,is_day"
                    .to_string(),
            ),
            (
                "daily",
                "temperature_2m_max,temperature_2m_min,uv_index_max,precipitation_probability_max"
                    .to_string(),
            ),
            ("timezone", "auto".to_string()),
            ("forecast_days", "1".to_string()),
        ],
    )
    .await?;
    let current = &forecast["current"];
    let daily = &forecast["daily"];
    let first = |key: &str| daily[key].get(0).cloned().unwrap_or(Value::Null);
    Ok(Some(json!({
        "place": place["name"],
        "region": place["admin1"],
        "temperature": current["temperature_2m"],
        "apparent": current["apparent_temperature"],
        "humidity": current["relative_humidity_2m"],
        "wind": current["wind_speed_10m"],
        "code": current["weather_code"],
        "is_day": current["is_day"],
        "max": first("temperature_2m_max"),
        "min": first("temperature_2m_min"),
        "uv": first("uv_index_max"),
        "rain_chance": first("precipitation_probability_max"),
    })))
}

async fn today_payload(city: String) -> Value {
    if city.is_empty() {
        return json!({ "city": "", "weather": null, "error": null });
    }
    if let Some(weather) = cached(&city) {
        return json!({ "city": city, "weather": weather, "error": null });
    }
    match fetch_weather(&city).await {
        Ok(Some(weather)) => {
            remember(&city, &weather);
            json!({ "city": city, "weather": weather, "error": null })
        }
        Ok(None) => json!({ "city": city, "weather": null, "error": "not_found" }),
        Err(error) => {
            tracing::debug!(error = %error, "today weather fetch failed");
            json!({ "city": city, "weather": null, "error": "unavailable" })
        }
    }
}

fn require_home_dir(
    state: &DaemonState,
    identity: &WebIdentity,
) -> std::result::Result<PathBuf, ApiError> {
    account_dir(&state.paths, identity)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "account directory not found"))
}

pub(in crate::web) async fn account_today_get(
    State(state): State<DaemonState>,
    headers: HeaderMap,
) -> std::result::Result<Response, ApiError> {
    let identity = require_identity(&headers, &state)?;
    let dir = require_home_dir(&state, &identity)?;
    let city = load_home(&dir).city;
    let mut response = Json(today_payload(city).await).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

pub(in crate::web) async fn account_today_put(
    State(state): State<DaemonState>,
    headers: HeaderMap,
    Json(request): Json<TodayRequest>,
) -> std::result::Result<Response, ApiError> {
    require_mutation(&headers, &state)?;
    let identity = require_identity(&headers, &state)?;
    let dir = require_home_dir(&state, &identity)?;
    let city = normalize_city(&request.city);
    save_home(&dir, &HomeSettings { city: city.clone() }).map_err(ApiError::internal)?;
    Ok(Json(today_payload(city).await).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn city_is_trimmed_and_bounded() {
        assert_eq!(normalize_city("  北京 \n"), "北京");
        assert_eq!(normalize_city("\u{7}上海\t"), "上海");
        assert_eq!(
            normalize_city(&"长".repeat(100)).chars().count(),
            MAX_CITY_CHARS
        );
        assert_eq!(normalize_city("   "), "");
    }

    #[test]
    fn home_settings_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        save_home(
            temp.path(),
            &HomeSettings {
                city: "杭州".to_string(),
            },
        )
        .unwrap();
        assert_eq!(load_home(temp.path()).city, "杭州");
        assert_eq!(load_home(&temp.path().join("missing")).city, "");
    }
}
