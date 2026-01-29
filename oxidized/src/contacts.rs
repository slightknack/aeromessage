//! Contact name resolution.

use std::collections::HashMap;

/// Resolves phone numbers and emails to contact names.
pub struct ContactResolver {
    cache: HashMap<String, String>,
}

impl ContactResolver {
    pub fn new() -> Self {
        Self { cache: HashMap::new() }
    }

    /// Get contact name for identifier (phone/email).
    pub fn resolve(&self, identifier: &str) -> Option<&str> {
        // Try direct lookup
        if let Some(name) = self.cache.get(identifier) {
            return Some(name);
        }

        // Try normalized phone
        let normalized = normalize_phone(identifier);
        if let Some(name) = self.cache.get(&normalized) {
            return Some(name);
        }

        // Try without +1 prefix
        if normalized.starts_with("+1") {
            if let Some(name) = self.cache.get(&normalized[2..]) {
                return Some(name);
            }
        }

        None
    }

    /// Add a mapping from identifier to name.
    pub fn add(&mut self, identifier: &str, name: &str) {
        if !identifier.is_empty() && !name.is_empty() {
            self.cache.insert(identifier.to_string(), name.to_string());
        }
    }

    /// Load contacts from macOS AddressBook database.
    pub fn load_macos_contacts(&mut self) -> Result<usize, String> {
        // Find AddressBook database
        let home = dirs::home_dir().ok_or("Cannot find home directory")?;
        let sources_dir = home.join("Library/Application Support/AddressBook/Sources");
        
        if !sources_dir.exists() {
            return Err("AddressBook sources directory not found".to_string());
        }
        
        let mut count = 0;
        
        // Iterate through all source directories
        let entries = std::fs::read_dir(&sources_dir)
            .map_err(|e| format!("Cannot read sources dir: {}", e))?;
        
        for entry in entries.flatten() {
            let db_path = entry.path().join("AddressBook-v22.abcddb");
            if !db_path.exists() {
                continue;
            }
            
            count += self.load_from_addressbook_db(&db_path)?;
        }
        
        Ok(count)
    }
    
    fn load_from_addressbook_db(&mut self, db_path: &std::path::Path) -> Result<usize, String> {
        use rusqlite::{Connection, OpenFlags};

        let conn = Connection::open_with_flags(
            db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ).map_err(|e| format!("Cannot open AddressBook: {}", e))?;
        
        let mut count = 0;
        
        // Load phone numbers
        let mut stmt = conn.prepare(
            "SELECT 
                COALESCE(r.ZFIRSTNAME, '') || ' ' || COALESCE(r.ZLASTNAME, '') as name,
                p.ZFULLNUMBER
            FROM ZABCDRECORD r
            JOIN ZABCDPHONENUMBER p ON r.Z_PK = p.ZOWNER
            WHERE (r.ZFIRSTNAME IS NOT NULL OR r.ZLASTNAME IS NOT NULL)
              AND p.ZFULLNUMBER IS NOT NULL"
        ).map_err(|e| format!("SQL error: {}", e))?;
        
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
            ))
        }).map_err(|e| format!("Query error: {}", e))?;
        
        for row in rows.flatten() {
            let (name, phone) = row;
            let name = name.trim();
            if !name.is_empty() && name != " " {
                self.add(&phone, name);
                let normalized = normalize_phone(&phone);
                if normalized != phone {
                    self.add(&normalized, name);
                }
                count += 1;
            }
        }
        
        // Load email addresses
        let mut stmt = conn.prepare(
            "SELECT 
                COALESCE(r.ZFIRSTNAME, '') || ' ' || COALESCE(r.ZLASTNAME, '') as name,
                e.ZADDRESSNORMALIZED
            FROM ZABCDRECORD r
            JOIN ZABCDEMAILADDRESS e ON r.Z_PK = e.ZOWNER
            WHERE (r.ZFIRSTNAME IS NOT NULL OR r.ZLASTNAME IS NOT NULL)
              AND e.ZADDRESSNORMALIZED IS NOT NULL"
        ).map_err(|e| format!("SQL error: {}", e))?;
        
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
            ))
        }).map_err(|e| format!("Query error: {}", e))?;
        
        for row in rows.flatten() {
            let (name, email) = row;
            let name = name.trim();
            if !name.is_empty() && name != " " {
                self.add(&email, name);
                // Also add lowercase version
                let lower = email.to_lowercase();
                if lower != email {
                    self.add(&lower, name);
                }
                count += 1;
            }
        }
        
        Ok(count)
    }
}

impl Default for ContactResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Normalize a phone number (keep only digits and +).
fn normalize_phone(phone: &str) -> String {
    phone.chars().filter(|c| c.is_ascii_digit() || *c == '+').collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_phone() {
        assert_eq!(normalize_phone("+1 (555) 123-4567"), "+15551234567");
        assert_eq!(normalize_phone("555.123.4567"), "5551234567");
    }

    #[test]
    fn test_resolver_direct() {
        let mut resolver = ContactResolver::new();
        resolver.add("+15551234567", "John Doe");

        assert_eq!(resolver.resolve("+15551234567"), Some("John Doe"));
        assert_eq!(resolver.resolve("+15559999999"), None);
    }

    #[test]
    fn test_resolver_normalized() {
        let mut resolver = ContactResolver::new();
        resolver.add("+15551234567", "Jane Doe");

        // Should find via normalization
        assert_eq!(resolver.resolve("+1 (555) 123-4567"), Some("Jane Doe"));
    }

    #[test]
    fn test_resolver_without_country_code() {
        let mut resolver = ContactResolver::new();
        resolver.add("5551234567", "Bob Smith");

        // Should find +1 prefixed version
        assert_eq!(resolver.resolve("+15551234567"), Some("Bob Smith"));
    }

    #[test]
    fn test_resolver_email() {
        let mut resolver = ContactResolver::new();
        resolver.add("john@example.com", "John Doe");

        assert_eq!(resolver.resolve("john@example.com"), Some("John Doe"));
        assert_eq!(resolver.resolve("other@example.com"), None);
    }

    #[test]
    fn test_resolver_empty_values() {
        let mut resolver = ContactResolver::new();
        resolver.add("", "No ID");
        resolver.add("+15551234567", "");
        resolver.add("", "");

        // Empty identifiers or names shouldn't be added
        assert_eq!(resolver.resolve(""), None);
        assert_eq!(resolver.resolve("+15551234567"), None);
    }

    #[test]
    fn test_resolver_default() {
        let resolver = ContactResolver::default();
        assert_eq!(resolver.resolve("+15551234567"), None);
    }

    #[test]
    fn test_normalize_phone_edge_cases() {
        assert_eq!(normalize_phone(""), "");
        assert_eq!(normalize_phone("abc"), "");
        assert_eq!(normalize_phone("+++123"), "+++123");
        assert_eq!(normalize_phone("  +1  555  "), "+1555");
    }
}
