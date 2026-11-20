use crate::models::OrganizationProfile;
use chrono::Local;
use std::env;
use std::fs;
use std::path::PathBuf;

/// Returns the storage path `%APPDATA%\MonWord\org_profiles.json`
/// (with fallback to `%LOCALAPPDATA%` or temp directory).
pub fn org_profiles_store_path() -> PathBuf {
    let base = env::var("APPDATA")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            env::var("LOCALAPPDATA")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir);
    base.join("MonWord").join("org_profiles.json")
}

/// Loads all saved Organization Profiles. Starts empty (`[]`) when no store file exists yet.
pub fn load_org_profiles() -> Vec<OrganizationProfile> {
    load_org_profiles_from(&org_profiles_store_path())
}

pub fn load_org_profiles_from(path: &PathBuf) -> Vec<OrganizationProfile> {
    let Ok(raw) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let cleaned = raw.trim_start_matches('\u{feff}');
    serde_json::from_str::<Vec<OrganizationProfile>>(cleaned).unwrap_or_default()
}

pub fn get_org_profile(id: &str) -> Option<OrganizationProfile> {
    let target = id.trim();
    if target.is_empty() {
        return None;
    }
    load_org_profiles()
        .into_iter()
        .find(|p| p.id == target)
}

fn save_org_profiles_to(path: &PathBuf, profiles: &[OrganizationProfile]) -> Result<(), String> {
    crate::engine::reliability::safe_save::save_json_atomic(path, &profiles)?;
    Ok(())
}

/// Inserts or updates an Organization Profile. Validates that issuing organization name is present.
pub fn upsert_org_profile(profile: OrganizationProfile) -> Result<Vec<OrganizationProfile>, String> {
    upsert_org_profile_at(&org_profiles_store_path(), profile)
}

pub fn upsert_org_profile_at(
    path: &PathBuf,
    mut profile: OrganizationProfile,
) -> Result<Vec<OrganizationProfile>, String> {
    let issuing = profile.issuing_org.trim().to_string();
    if issuing.is_empty() {
        return Err("Tên cơ quan, tổ chức ban hành văn bản không được để trống.".to_string());
    }
    profile.issuing_org = issuing.clone();
    profile.governing_org = profile.governing_org.trim().to_string();
    profile.affiliated_unit = profile.affiliated_unit.trim().to_string();
    profile.department_or_branch = profile.department_or_branch.trim().to_string();
    profile.agency_abbreviation = profile.agency_abbreviation.trim().to_string();
    profile.default_location = profile.default_location.trim().to_string();
    if profile.profile_name.trim().is_empty() {
        profile.profile_name = issuing;
    } else {
        profile.profile_name = profile.profile_name.trim().to_string();
    }
    if profile.id.trim().is_empty() {
        profile.id = format!(
            "org_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(1)
        );
    }
    profile.updated_at = Local::now().format("%d/%m/%Y %H:%M:%S").to_string();

    let mut list = load_org_profiles_from(path);
    if let Some(existing) = list.iter_mut().find(|p| p.id == profile.id) {
        *existing = profile;
    } else {
        list.push(profile);
    }
    save_org_profiles_to(path, &list)?;
    Ok(list)
}

pub fn delete_org_profile(id: &str) -> Result<Vec<OrganizationProfile>, String> {
    delete_org_profile_at(&org_profiles_store_path(), id)
}

pub fn delete_org_profile_at(path: &PathBuf, id: &str) -> Result<Vec<OrganizationProfile>, String> {
    let mut list = load_org_profiles_from(path);
    let before = list.len();
    list.retain(|p| p.id != id);
    if list.len() == before {
        return Err("Không tìm thấy hồ sơ tổ chức cần xóa.".to_string());
    }
    save_org_profiles_to(path, &list)?;
    Ok(list)
}

pub fn export_org_profiles_json() -> Result<String, String> {
    let list = load_org_profiles();
    serde_json::to_string_pretty(&list)
        .map_err(|e| format!("Lỗi xuất JSON hồ sơ tổ chức: {}", e))
}

pub fn import_org_profiles_json(raw_json: &str) -> Result<Vec<OrganizationProfile>, String> {
    import_org_profiles_json_at(&org_profiles_store_path(), raw_json)
}

pub fn import_org_profiles_json_at(
    path: &PathBuf,
    raw_json: &str,
) -> Result<Vec<OrganizationProfile>, String> {
    let cleaned = raw_json.trim_start_matches('\u{feff}');
    let incoming: Vec<OrganizationProfile> = serde_json::from_str(cleaned)
        .map_err(|e| format!("Dữ liệu JSON hồ sơ tổ chức không hợp lệ: {}", e))?;
    let mut current = load_org_profiles_from(path);
    for mut item in incoming {
        if item.issuing_org.trim().is_empty() {
            continue;
        }
        if item.id.trim().is_empty() {
            item.id = format!(
                "org_imp_{}_{}",
                current.len() + 1,
                Local::now().timestamp_millis()
            );
        }
        if item.profile_name.trim().is_empty() {
            item.profile_name = item.issuing_org.trim().to_string();
        }
        item.updated_at = Local::now().format("%d/%m/%Y %H:%M:%S").to_string();
        if let Some(slot) = current.iter_mut().find(|x| x.id == item.id) {
            *slot = item;
        } else {
            current.push(item);
        }
    }
    save_org_profiles_to(path, &current)?;
    Ok(current)
}
