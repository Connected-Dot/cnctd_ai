use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct EntityRecord {
    pub entity_type: String,
    pub id: String,
    pub name: String,
}

pub struct EntityDictionary {
    /// name (lowercase) -> records with that name
    name_to_records: HashMap<String, Vec<EntityRecord>>,
    /// (type, id) -> record
    id_to_record: HashMap<(String, String), EntityRecord>,
    /// All distinct entity type names
    entity_types: Vec<String>,
}

impl EntityDictionary {
    pub fn new() -> Self {
        Self {
            name_to_records: HashMap::new(),
            id_to_record: HashMap::new(),
            entity_types: Vec::new(),
        }
    }

    pub fn load(&mut self, records: Vec<EntityRecord>) {
        self.name_to_records.clear();
        self.id_to_record.clear();

        let mut type_set = std::collections::HashSet::new();

        for record in records {
            type_set.insert(record.entity_type.clone());
            let lower_name = record.name.to_lowercase();
            self.name_to_records
                .entry(lower_name)
                .or_default()
                .push(record.clone());
            self.id_to_record
                .insert((record.entity_type.clone(), record.id.clone()), record);
        }

        self.entity_types = type_set.into_iter().collect();
        self.entity_types.sort();
    }

    pub fn lookup_by_name(&self, name: &str) -> Option<&[EntityRecord]> {
        self.name_to_records
            .get(&name.to_lowercase())
            .map(|v| v.as_slice())
    }

    pub fn lookup_by_id(&self, entity_type: &str, id: &str) -> Option<&EntityRecord> {
        self.id_to_record
            .get(&(entity_type.to_string(), id.to_string()))
    }

    pub fn all_names(&self) -> Vec<&str> {
        self.name_to_records.keys().map(|s| s.as_str()).collect()
    }

    pub fn all_records(&self) -> impl Iterator<Item = &EntityRecord> {
        self.id_to_record.values()
    }

    /// All distinct entity type names found in the dictionary.
    pub fn entity_types(&self) -> &[String] {
        &self.entity_types
    }

    pub fn stats(&self) -> HashMap<String, usize> {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for record in self.id_to_record.values() {
            *counts.entry(record.entity_type.clone()).or_default() += 1;
        }
        counts
    }
}
