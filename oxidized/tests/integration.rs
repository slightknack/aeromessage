//! Integration tests against real chat.db (requires Full Disk Access).

use aeromessage::Database;

#[test]
fn test_open_real_database() {
    let path = Database::default_path();
    
    // Skip if database doesn't exist (CI environment)
    if !path.exists() {
        eprintln!("Skipping: chat.db not found at {:?}", path);
        return;
    }

    match Database::open(&path) {
        Ok(_db) => {
            println!("Successfully opened chat.db");
        }
        Err(e) => {
            // Permission denied is expected without FDA
            eprintln!("Could not open database: {}", e);
        }
    }
}

#[test]
fn test_fetch_unread_conversations() {
    let path = Database::default_path();
    
    if !path.exists() {
        eprintln!("Skipping: chat.db not found");
        return;
    }

    let db = match Database::open(&path) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping: {}", e);
            return;
        }
    };

    match db.unread_conversations() {
        Ok(convs) => {
            println!("Found {} unread conversations", convs.len());
            for conv in convs.iter().take(3) {
                println!(
                    "  - {} ({} unread, {} messages)",
                    conv.name(),
                    conv.unread_count,
                    conv.messages.len()
                );
            }
        }
        Err(e) => {
            eprintln!("Error fetching conversations: {}", e);
        }
    }
}
