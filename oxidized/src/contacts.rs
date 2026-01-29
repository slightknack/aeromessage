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

    /// Load contacts from macOS Contacts using AppleScript.
    pub fn load_macos_contacts(&mut self) -> Result<usize, String> {
        use std::process::Command;
        
        let script = r#"
            set output to ""
            tell application "Contacts"
                repeat with p in every person
                    set pName to name of p
                    repeat with ph in phones of p
                        set output to output & pName & "	" & value of ph & linefeed
                    end repeat
                    repeat with em in emails of p
                        set output to output & pName & "	" & value of em & linefeed
                    end repeat
                end repeat
            end tell
            return output
        "#;
        
        let output = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .map_err(|e| format!("Failed to run osascript: {}", e))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("AppleScript error: {}", stderr));
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut count = 0;
        
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 2 {
                let name = parts[0].trim();
                let identifier = parts[1].trim();
                if !name.is_empty() && !identifier.is_empty() {
                    // Add both raw and normalized versions
                    self.add(identifier, name);
                    let normalized = normalize_phone(identifier);
                    if normalized != identifier {
                        self.add(&normalized, name);
                    }
                    count += 1;
                }
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
}
