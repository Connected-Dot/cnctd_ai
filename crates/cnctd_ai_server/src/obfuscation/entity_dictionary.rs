use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityType {
    Channel,
    Bidder,
    Advertiser,
    Agency,
    Order,
    LineItem,
    Trafficker,
}

impl EntityType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EntityType::Channel => "channel",
            EntityType::Bidder => "bidder",
            EntityType::Advertiser => "advertiser",
            EntityType::Agency => "agency",
            EntityType::Order => "order",
            EntityType::LineItem => "line_item",
            EntityType::Trafficker => "trafficker",
        }
    }

    pub fn from_str(s: &str) -> Option<EntityType> {
        match s {
            "channel" => Some(EntityType::Channel),
            "bidder" => Some(EntityType::Bidder),
            "advertiser" => Some(EntityType::Advertiser),
            "agency" => Some(EntityType::Agency),
            "order" => Some(EntityType::Order),
            "line_item" => Some(EntityType::LineItem),
            "trafficker" => Some(EntityType::Trafficker),
            _ => None,
        }
    }

    pub fn all() -> &'static [EntityType] {
        &[
            EntityType::Channel,
            EntityType::Bidder,
            EntityType::Advertiser,
            EntityType::Agency,
            EntityType::Order,
            EntityType::LineItem,
            EntityType::Trafficker,
        ]
    }
}

impl fmt::Display for EntityType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct EntityRecord {
    pub entity_type: EntityType,
    pub id: i32,
    pub name: String,
}

pub struct EntityDictionary {
    /// name (lowercase) -> records with that name
    name_to_records: HashMap<String, Vec<EntityRecord>>,
    /// (type, id) -> record
    id_to_record: HashMap<(EntityType, i32), EntityRecord>,
}

impl EntityDictionary {
    pub fn new() -> Self {
        Self {
            name_to_records: HashMap::new(),
            id_to_record: HashMap::new(),
        }
    }

    pub fn load(&mut self, records: Vec<EntityRecord>) {
        self.name_to_records.clear();
        self.id_to_record.clear();

        for record in records {
            let lower_name = record.name.to_lowercase();
            self.name_to_records
                .entry(lower_name)
                .or_default()
                .push(record.clone());
            self.id_to_record
                .insert((record.entity_type, record.id), record);
        }
    }

    pub fn lookup_by_name(&self, name: &str) -> Option<&[EntityRecord]> {
        self.name_to_records
            .get(&name.to_lowercase())
            .map(|v| v.as_slice())
    }

    pub fn lookup_by_id(&self, entity_type: EntityType, id: i32) -> Option<&EntityRecord> {
        self.id_to_record.get(&(entity_type, id))
    }

    pub fn all_names(&self) -> Vec<&str> {
        // Return original-case names from id_to_record (deduplicated via the map keys)
        self.name_to_records.keys().map(|s| s.as_str()).collect()
    }

    pub fn all_records(&self) -> impl Iterator<Item = &EntityRecord> {
        self.id_to_record.values()
    }

    pub fn stats(&self) -> HashMap<EntityType, usize> {
        let mut counts: HashMap<EntityType, usize> = HashMap::new();
        for record in self.id_to_record.values() {
            *counts.entry(record.entity_type).or_default() += 1;
        }
        counts
    }
}

// Postgres entity table definitions
struct EntityTable {
    entity_type: EntityType,
    table: &'static str,
    id_column: &'static str,
    name_expr: &'static str,
}

const ENTITY_TABLES: &[EntityTable] = &[
    EntityTable {
        entity_type: EntityType::Channel,
        table: "channels",
        id_column: "id",
        name_expr: "name",
    },
    EntityTable {
        entity_type: EntityType::Bidder,
        table: "bidders",
        id_column: "id",
        name_expr: "name",
    },
    EntityTable {
        entity_type: EntityType::Advertiser,
        table: "advertisers",
        id_column: "id",
        name_expr: "name",
    },
    EntityTable {
        entity_type: EntityType::Agency,
        table: "agencies",
        id_column: "id",
        name_expr: "name",
    },
    EntityTable {
        entity_type: EntityType::Order,
        table: "orders",
        id_column: "id",
        name_expr: "name",
    },
    EntityTable {
        entity_type: EntityType::LineItem,
        table: "line_items",
        id_column: "id",
        name_expr: "name",
    },
    EntityTable {
        entity_type: EntityType::Trafficker,
        table: "traffickers",
        id_column: "id",
        name_expr: "CONCAT(first_name, ' ', last_name)",
    },
];

pub async fn load_from_postgres(
    conn_string: &str,
) -> Result<Vec<EntityRecord>, Box<dyn std::error::Error + Send + Sync>> {
    let (client, connection) = tokio_postgres::connect(conn_string, tokio_postgres::NoTls).await?;

    // Spawn connection handler
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            tracing::error!("Postgres connection error: {e}");
        }
    });

    let mut records = Vec::new();

    for table in ENTITY_TABLES {
        let query = format!(
            "SELECT {} AS id, {} AS name FROM {}",
            table.id_column, table.name_expr, table.table
        );
        match client.query(&query, &[]).await {
            Ok(rows) => {
                for row in &rows {
                    let id: i32 = row.get(0);
                    let name: String = row.get(1);
                    records.push(EntityRecord {
                        entity_type: table.entity_type,
                        id,
                        name,
                    });
                }
            }
            Err(e) => {
                tracing::warn!("Skipping {}: {e}", table.table);
            }
        }
    }

    Ok(records)
}
