use aho_corasick::AhoCorasick;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";
const MAX_ZIP_ENTRY_SIZE: u64 = 100 * 1024 * 1024; // 100 MB per ZIP entry

fn read_zip_entry_safe(entry: &mut zip::read::ZipFile) -> Result<Vec<u8>, String> {
    if entry.size() > MAX_ZIP_ENTRY_SIZE {
        return Err(format!(
            "Plik ZIP zawiera zbyt duży element: {} ({:.1} MB, limit {:.0} MB)",
            entry.name(), entry.size() as f64 / 1_048_576.0, MAX_ZIP_ENTRY_SIZE as f64 / 1_048_576.0
        ));
    }
    use std::io::Read;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf)
        .map_err(|e| format!("Błąd odczytu {}: {}", entry.name(), e))?;
    Ok(buf)
}

// Category → NER types mapping (controls which entities are tokenized)
const CATEGORY_TYPES: &[(&str, &[&str])] = &[
    ("PERSON",  &["PERSON"]),
    ("ADDRESS", &["ADDRESS"]),
    ("COMPANY", &["COMPANY"]),
    ("ID",      &["NIP", "REGON", "KRS", "PESEL", "CONTRACT_ID", "OTHER_ID"]),
    ("AMOUNT",  &["AMOUNT", "BANK_ACCOUNT"]),
    ("CONTACT", &["PHONE", "EMAIL"]),
    ("DATE",    &["DATE"]),
];

// NER type → description (for prompt building)
const TYPE_DESCRIPTIONS: &[(&str, &str)] = &[
    ("PERSON",      "PERSON — imiona i nazwiska osób (np. \"Jan Kowalski\", \"Anna Nowak-Wiśniewska\")"),
    ("COMPANY",     "COMPANY — nazwy firm, spółek, instytucji (np. \"ABC Sp. z o.o.\", \"Urząd Miasta Krakowa\")"),
    ("AMOUNT",      "AMOUNT — kwoty pieniężne w KAŻDEJ formie:\n    • liczbowe: \"8 934 000,00 PLN\", \"1.5 mln zł\", \"123,45 EUR\", \"50 000,00 zł\"\n    • słowne: \"pięćdziesiąt tysięcy złotych\", \"dwa miliony trzysta tysięcy 00/100\"\n    • mieszane: \"50 000,00 zł (słownie: pięćdziesiąt tysięcy złotych 00/100)\" — traktuj jako JEDEN obiekt\n    • procenty od kwot: \"2% wartości umowy\" NIE jest kwotą — ignoruj"),
    ("DATE",        "DATE — konkretne daty w KAŻDYM formacie:\n    • \"15 marca 2026 r.\", \"15.03.2026\", \"2026-03-15\"\n    • \"dnia 15 marca 2026 roku\", \"z dnia 10.01.2026 r.\"\n    • \"do dnia 31.12.2026\", \"w terminie do 30 czerwca 2026 r.\"\n    • \"od 01.01.2026 do 31.12.2026\" — to DWA osobne obiekty DATE\n    • NIE oznaczaj ogólników: \"w ciągu 14 dni\", \"30 dni roboczych\" to NIE są daty"),
    ("ADDRESS",     "ADDRESS — pełne adresy (np. \"ul. Kwiatowa 15, 00-001 Warszawa\")"),
    ("PHONE",       "PHONE — numery telefonów (np. \"+48 123 456 789\", \"123-456-789\")"),
    ("EMAIL",       "EMAIL — adresy email"),
    ("CONTRACT_ID", "CONTRACT_ID — numery umów, postępowań, sygnatur (np. \"ZP/385/LZA/2025\", \"DZP.26.1.2025\")"),
    ("NIP",         "NIP — numery NIP (np. \"NIP: 123-456-78-90\", \"NIP 1234567890\")"),
    ("REGON",       "REGON — numery REGON"),
    ("KRS",         "KRS — numery KRS"),
    ("BANK_ACCOUNT","BANK_ACCOUNT — numery kont bankowych (np. \"PL 12 3456 7890 1234 5678 9012 3456\")"),
    ("PESEL",       "PESEL — numery PESEL (11 cyfr)"),
    ("OTHER_ID",    "OTHER_ID — inne identyfikatory (nr dowodu osobistego np. \"ABC123456\", nr ARiMR, numery ewidencyjne)"),
];

fn build_ner_prompt(text: &str, enabled_categories: &Option<Vec<String>>) -> (String, Vec<String>) {
    // Resolve categories to allowed NER types (for post-hoc filtering)
    let allowed_types: Vec<String> = match enabled_categories {
        Some(cats) => {
            let mut types = Vec::new();
            for (cat, ner_types) in CATEGORY_TYPES {
                if cats.iter().any(|c| c == cat) {
                    for t in *ner_types {
                        types.push(t.to_string());
                    }
                }
            }
            types
        }
        None => TYPE_DESCRIPTIONS.iter().map(|(t, _)| t.to_string()).collect(),
    };

    // Prompt ALWAYS contains all types — better classification accuracy
    let type_lines: Vec<String> = TYPE_DESCRIPTIONS.iter()
        .map(|(_, desc)| format!("- {}", desc))
        .collect();

    let prompt = format!(
r#"Jesteś ekspertem od anonimizacji dokumentów urzędowych i umów. Twoim zadaniem jest znalezienie WSZYSTKICH danych wrażliwych w tekście.

Zwróć TYLKO JSON array z obiektami, bez żadnego dodatkowego tekstu, komentarzy ani markdown.
Każdy obiekt musi mieć pola: "text" (dokładny tekst z dokumentu), "type" (typ encji).

## Typy encji:
{}

## Zasady:
1. Znajdź KAŻDE wystąpienie — nawet jeśli ta sama encja pojawia się wielokrotnie, wypisz ją RAZ
2. Zachowaj DOKŁADNY tekst z dokumentu (wielkość liter, spacje, interpunkcja)
3. Nie łącz różnych encji — "Jan Kowalski z Firma Sp. z o.o." to DWA obiekty
4. NIE anonimizuj nazw ogólnych: "Zamawiający", "Wykonawca", "Strona", "Polska"
5. NIE anonimizuj terminów prawnych, technicznych, nazw produktów, standardów
6. Kwota z częścią słowną w nawiasie to JEDEN obiekt — nie dziel na dwa

## Najczęściej pomijane — zwróć szczególną uwagę:
- Kwoty zapisane SŁOWNIE (np. "dwieście pięćdziesiąt tysięcy złotych")
- Daty z polskim formatowaniem (np. "dnia 15 marca 2026 r.")
- Numery umów/postępowań ukryte w treści (np. "na podstawie umowy ZP/123/2025")
- Numery kont bankowych (26 cyfr, czasem z PL na początku)

## Przykład:
Tekst: "Firma XYZ Sp. z o.o. (NIP: 987-654-32-10) z siedzibą przy ul. Leśnej 5, 30-001 Kraków zobowiązuje się zapłacić kwotę 250 000,00 zł (słownie: dwieście pięćdziesiąt tysięcy złotych 00/100) na konto nr PL 61 1090 1014 0000 0712 1981 2874 w terminie do dnia 30 czerwca 2026 r. Osoba do kontaktu: Maria Wiśniewska, tel. 512 345 678."

Odpowiedź:
[
  {{"text": "XYZ Sp. z o.o.", "type": "COMPANY"}},
  {{"text": "987-654-32-10", "type": "NIP"}},
  {{"text": "ul. Leśnej 5, 30-001 Kraków", "type": "ADDRESS"}},
  {{"text": "250 000,00 zł (słownie: dwieście pięćdziesiąt tysięcy złotych 00/100)", "type": "AMOUNT"}},
  {{"text": "PL 61 1090 1014 0000 0712 1981 2874", "type": "BANK_ACCOUNT"}},
  {{"text": "30 czerwca 2026 r.", "type": "DATE"}},
  {{"text": "Maria Wiśniewska", "type": "PERSON"}},
  {{"text": "512 345 678", "type": "PHONE"}}
]

## Tekst do analizy:
---
{}
---

Odpowiedz TYLKO validnym JSON array:"#, type_lines.join("\n"), text);

    (prompt, allowed_types)
}

// --- Data structures ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityInfo {
    pub original: String,
    pub token: String,
    pub entity_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnonymizeResult {
    pub text: String,
    pub entities_found: usize,
    pub model_used: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnonMap {
    pub meta: AnonMapMeta,
    pub entities: Vec<EntityInfo>,
    pub reverse: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnonMapMeta {
    pub source_file: String,
    pub created: String,
    pub model: String,
    pub entities_count: usize,
    pub thaler_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeanonStats {
    pub total: usize,
    pub found: usize,
    pub missing: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct NerEntity {
    text: String,
    #[serde(rename = "type")]
    entity_type: String,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    response: Option<String>,
}

// --- File readers ---

pub fn read_file(path: &str) -> Result<String, String> {
    let p = Path::new(path);
    let ext = p.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "xlsx" | "xls" => read_xlsx(path),
        "docx" => read_docx(path),
        "md" | "txt" | "csv" => std::fs::read_to_string(path)
            .map_err(|e| format!("Błąd odczytu {}: {}", path, e)),
        _ => Err(format!("Nieobsługiwany format: .{}", ext)),
    }
}

fn read_xlsx(path: &str) -> Result<String, String> {
    use calamine::{Reader, open_workbook, Xlsx};

    let mut workbook: Xlsx<_> = open_workbook(path)
        .map_err(|e| format!("Nie mogę otworzyć XLSX: {}", e))?;

    let mut output = String::new();

    for sheet_name in workbook.sheet_names().to_vec() {
        output.push_str(&format!("# {}\n\n", sheet_name));

        if let Ok(range) = workbook.worksheet_range(&sheet_name) {
            for row in range.rows() {
                let cells: Vec<String> = row.iter().map(|cell| {
                    match cell {
                        calamine::Data::Empty => String::new(),
                        calamine::Data::String(s) => s.clone(),
                        calamine::Data::Float(f) => format!("{}", f),
                        calamine::Data::Int(i) => format!("{}", i),
                        calamine::Data::Bool(b) => format!("{}", b),
                        calamine::Data::DateTime(dt) => format!("{}", dt),
                        calamine::Data::Error(e) => format!("ERR:{:?}", e),
                        _ => String::from("?"),
                    }
                }).collect();

                if cells.iter().any(|c| !c.is_empty()) {
                    output.push_str("| ");
                    output.push_str(&cells.join(" | "));
                    output.push_str(" |\n");
                }
            }
        }
        output.push('\n');
    }

    Ok(output)
}

fn read_docx(path: &str) -> Result<String, String> {
    use std::io::Read;

    let file = std::fs::File::open(path)
        .map_err(|e| format!("Nie mogę otworzyć DOCX: {}", e))?;

    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("DOCX nie jest poprawnym ZIP: {}", e))?;

    let mut xml_content = String::new();
    {
        let mut doc_file = archive.by_name("word/document.xml")
            .map_err(|e| format!("Brak word/document.xml w DOCX: {}", e))?;
        if doc_file.size() > MAX_ZIP_ENTRY_SIZE {
            return Err(format!("word/document.xml zbyt duży: {:.1} MB", doc_file.size() as f64 / 1_048_576.0));
        }
        doc_file.read_to_string(&mut xml_content)
            .map_err(|e| format!("Błąd odczytu XML z DOCX: {}", e))?;
    }

    // Parse XML and extract text
    let mut output = String::new();
    let mut reader = quick_xml::Reader::from_str(&xml_content);
    let mut in_text = false;
    let mut current_paragraph = String::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(ref e)) => {
                match e.name().as_ref() {
                    b"w:t" => in_text = true,
                    b"w:p" => current_paragraph.clear(),
                    _ => {}
                }
            }
            Ok(quick_xml::events::Event::Empty(ref e)) => {
                // Self-closing tags: <w:br/>, <w:tab/>
                match e.name().as_ref() {
                    b"w:br" => current_paragraph.push('\n'),
                    b"w:tab" => current_paragraph.push('\t'),
                    _ => {}
                }
            }
            Ok(quick_xml::events::Event::Text(ref e)) => {
                if in_text {
                    if let Ok(text) = e.unescape() {
                        current_paragraph.push_str(&text);
                    }
                }
            }
            Ok(quick_xml::events::Event::End(ref e)) => {
                match e.name().as_ref() {
                    b"w:t" => in_text = false,
                    b"w:p" => {
                        if !current_paragraph.trim().is_empty() {
                            output.push_str(current_paragraph.trim());
                            output.push('\n');
                        }
                    }
                    _ => {}
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(e) => return Err(format!("Błąd parsowania DOCX XML: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(output)
}

// --- Anonymizer ---

fn type_to_polish(t: &str) -> &str {
    match t {
        "PERSON" => "OSOBA",
        "COMPANY" => "FIRMA",
        "AMOUNT" => "KWOTA",
        "DATE" => "DATA",
        "ADDRESS" => "ADRES",
        "PHONE" => "TELEFON",
        "EMAIL" => "EMAIL",
        "CONTRACT_ID" => "UMOWA",
        "NIP" => "NIP",
        "REGON" => "REGON",
        "KRS" => "KRS",
        "BANK_ACCOUNT" => "KONTO",
        "PESEL" => "PESEL",
        "OTHER_ID" => "ID",
        _ => t,  // dynamic: model-invented types become token names
    }
}

pub struct Anonymizer {
    client: Client,
    ollama_url: String,
    primary_model: String,
    entities: HashMap<String, EntityInfo>,
    reverse: HashMap<String, String>,
    counters: HashMap<String, u32>,
    last_model_used: String,
    last_source_file: String,
    log_sink: Option<std::sync::Arc<std::sync::Mutex<Vec<String>>>>,
    /// Original file bytes — kept for native format export (DOCX→DOCX, XLSX→XLSX)
    original_file_bytes: Option<Vec<u8>>,
    original_file_ext: String,
}

impl Anonymizer {
    pub fn new() -> Self {
        let ollama_url = std::env::var("OLLAMA_URL")
            .unwrap_or_else(|_| DEFAULT_OLLAMA_URL.to_string());

        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            client,
            ollama_url,
            primary_model: String::new(),
            entities: HashMap::new(),
            reverse: HashMap::new(),
            counters: HashMap::new(),
            last_model_used: String::new(),
            last_source_file: String::new(),
            log_sink: None,
            original_file_bytes: None,
            original_file_ext: String::new(),
        }
    }

    pub fn set_log_sink(&mut self, sink: std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        self.log_sink = Some(sink);
    }

    pub fn clear_log_sink(&mut self) {
        self.log_sink = None;
    }

    fn log(&self, msg: &str) {
        let ts = chrono::Local::now().format("%H:%M:%S").to_string();
        let entry = format!("[{}] {}", ts, msg);
        eprintln!("{}", entry);
        if let Some(ref sink) = self.log_sink {
            if let Ok(mut logs) = sink.lock() {
                logs.push(entry);
            }
        }
    }

    pub fn set_config(&mut self, url: String, model: String) {
        if !url.is_empty() {
            self.ollama_url = url;
        }
        if !model.is_empty() {
            self.primary_model = model;
        }
    }

    pub fn get_ollama_url(&self) -> &str {
        &self.ollama_url
    }

    pub fn get_primary_model(&self) -> &str {
        &self.primary_model
    }

    pub async fn list_models(&self) -> Result<Vec<String>, String> {
        let url = format!("{}/api/tags", self.ollama_url);
        let resp = self.client.get(&url).send().await
            .map_err(|e| format!("Brak połączenia z Ollama: {}", e))?;

        let data: serde_json::Value = resp.json().await
            .map_err(|e| format!("Błąd parsowania odpowiedzi: {}", e))?;

        let models = data.get("models")
            .and_then(|m| m.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m.get("name").and_then(|n| n.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Ok(models)
    }

    pub async fn check_connection(&self) -> Result<String, String> {
        let url = format!("{}/api/tags", self.ollama_url);
        match self.client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => Ok("OK".to_string()),
            Ok(resp) => Err(format!("Ollama HTTP {}", resp.status())),
            Err(e) => Err(format!("Brak połączenia z Ollama: {}", e)),
        }
    }

    async fn call_ollama(&self, prompt: &str, model: &str) -> Result<String, String> {
        let url = format!("{}/api/chat", self.ollama_url);

        let body = serde_json::json!({
            "model": model,
            "messages": [
                {
                    "role": "system",
                    "content": "You are a document anonymization expert specialized in Polish legal and business documents. You find ALL sensitive entities and return them as a JSON array. Respond ONLY with valid JSON, no markdown, no commentary. Pay special attention to amounts written as words and dates in Polish format."
                },
                {
                    "role": "user",
                    "content": prompt
                }
            ],
            "stream": false,
            "options": {
                "temperature": 0.1,
                "num_predict": 4096
            }
        });

        let resp = self.client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Ollama request failed: {}", e))?;

        let data: serde_json::Value = resp.json().await
            .map_err(|e| format!("Ollama parse failed: {}", e))?;

        data["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| "Brak odpowiedzi z Ollama".to_string())
    }

    fn extract_json(text: &str) -> Vec<NerEntity> {
        let text = text.trim();

        // Try 1: whole text is JSON array
        if let Ok(entities) = serde_json::from_str::<Vec<NerEntity>>(text) {
            return entities;
        }

        // Try 2: extract [...] from text (e.g. markdown code block)
        if let Some(start) = text.find('[') {
            if let Some(end) = text.rfind(']') {
                let slice = &text[start..=end];
                if let Ok(entities) = serde_json::from_str::<Vec<NerEntity>>(slice) {
                    return entities;
                }
            }

            // Try 3: incomplete JSON — truncated response, missing closing }]
            // Find the last complete object (ends with }) and close the array
            let from_bracket = &text[start..];
            let last_brace = from_bracket.rfind('}');
            if let Some(pos) = last_brace {
                let repaired = format!("{}]", &from_bracket[..=pos]);
                if let Ok(entities) = serde_json::from_str::<Vec<NerEntity>>(&repaired) {
                    return entities;
                }
            }
        }

        // Try 4: objects without wrapping array — wrap them
        // Bielik sometimes returns {}, {} without [ ]
        if text.contains("\"text\"") && text.contains("\"type\"") {
            let cleaned = text.replace("```json", "").replace("```", "");
            let trimmed = cleaned.trim().trim_end_matches(',');
            let wrapped = format!("[{}]", trimmed);
            if let Ok(entities) = serde_json::from_str::<Vec<NerEntity>>(&wrapped) {
                return entities;
            }
        }

        // Try 5: trailing comma inside [...] — strip it before parsing
        if let Some(start) = text.find('[') {
            let from_bracket = &text[start..];
            // Find last } and wrap to ]
            if let Some(last_brace) = from_bracket.rfind('}') {
                let inner = &from_bracket[1..=last_brace]; // skip leading [
                let trimmed = inner.trim().trim_end_matches(',');
                let repaired = format!("[{}]", trimmed);
                if let Ok(entities) = serde_json::from_str::<Vec<NerEntity>>(&repaired) {
                    return entities;
                }
            }
        }

        // Try 6: regex — extract individual {"text":"...","type":"..."} objects
        let re = regex::Regex::new(
            r#"\{\s*"text"\s*:\s*"([^"\\]*(?:\\.[^"\\]*)*)"\s*,\s*"type"\s*:\s*"([^"\\]*(?:\\.[^"\\]*)*)"\s*\}"#
        ).unwrap();
        let mut results = Vec::new();
        for cap in re.captures_iter(text) {
            if let (Some(t), Some(ty)) = (cap.get(1), cap.get(2)) {
                results.push(NerEntity {
                    text: t.as_str().replace("\\\"", "\"").replace("\\\\", "\\"),
                    entity_type: ty.as_str().to_string(),
                });
            }
        }
        if !results.is_empty() {
            eprintln!("[extract_json] regex fallback: {} encji", results.len());
            return results;
        }

        Vec::new()
    }

    fn chunk_text(text: &str, max_chars: usize) -> Vec<String> {
        // Phase 1: split on paragraph boundaries (\n\n)
        let paragraphs: Vec<&str> = text.split("\n\n").collect();
        let mut coarse_chunks = Vec::new();
        let mut current = String::new();

        for para in paragraphs {
            if !current.is_empty() && current.len() + para.len() + 2 > max_chars {
                coarse_chunks.push(current.clone());
                current = para.to_string();
            } else {
                if !current.is_empty() {
                    current.push_str("\n\n");
                }
                current.push_str(para);
            }
        }

        if !current.trim().is_empty() {
            coarse_chunks.push(current);
        }

        // Phase 2: split oversized chunks on single newlines (handles XLSX/CSV rows)
        let mut chunks = Vec::new();
        for chunk in coarse_chunks {
            if chunk.len() <= max_chars {
                chunks.push(chunk);
            } else {
                let lines: Vec<&str> = chunk.split('\n').collect();
                let mut sub = String::new();
                for line in lines {
                    if !sub.is_empty() && sub.len() + line.len() + 1 > max_chars {
                        chunks.push(sub.clone());
                        sub = line.to_string();
                    } else {
                        if !sub.is_empty() {
                            sub.push('\n');
                        }
                        sub.push_str(line);
                    }
                }
                if !sub.trim().is_empty() {
                    chunks.push(sub);
                }
            }
        }

        if chunks.is_empty() {
            chunks.push(text.to_string());
        }

        chunks
    }

    fn get_or_create_token(&mut self, text: &str, entity_type: &str) -> String {
        let key = text.trim().to_string();

        if let Some(info) = self.entities.get(&key) {
            return info.token.clone();
        }

        let key_lower = key.to_lowercase();
        for (existing_key, info) in &self.entities {
            if existing_key.to_lowercase() == key_lower && info.entity_type == entity_type {
                let token = info.token.clone();
                self.entities.insert(key.clone(), info.clone());
                self.reverse.insert(token.clone(), key);
                return token;
            }
        }

        let pl_type = type_to_polish(entity_type).replace(' ', "_");
        let counter = self.counters.entry(pl_type.clone()).or_insert(0);
        *counter += 1;
        let token = format!("[TH_{}_{:03}]", pl_type, counter);

        let info = EntityInfo {
            original: key.clone(),
            token: token.clone(),
            entity_type: entity_type.to_string(),
        };

        self.entities.insert(key.clone(), info);
        self.reverse.insert(token.clone(), key);

        token
    }

    /// Sanitize text for safe insertion into LLM prompt.
    /// Removes control characters, null bytes, and normalizes whitespace
    /// while preserving document structure (\n, \t).
    fn sanitize_for_model(text: &str) -> String {
        text.chars()
            .filter(|c| {
                // Keep printable chars, newlines, tabs
                // Remove null bytes, control chars, BOM, etc.
                !c.is_control() || *c == '\n' || *c == '\t'
            })
            .collect::<String>()
            // Collapse 3+ consecutive newlines to 2
            .replace("\n\n\n", "\n\n")
            // Remove the prompt delimiter if it appears in user text
            .replace("\n---\n", "\n— — —\n")
    }

    pub async fn anonymize(&mut self, text: &str, source_file: &str, categories: Option<Vec<String>>) -> Result<AnonymizeResult, String> {
        self.clear();
        self.last_source_file = source_file.to_string();

        if self.primary_model.is_empty() {
            return Err("Nie wybrano modelu — kliknij Sprawdź i wybierz model z listy".to_string());
        }

        // Sanitize input text for model
        let clean_text = Self::sanitize_for_model(text);
        if clean_text.len() != text.len() {
            self.log(&format!("Sanityzacja: {} → {} znaków (usunięto znaki kontrolne)", text.len(), clean_text.len()));
        }

        let chunks = Self::chunk_text(&clean_text, 3000);
        let mut all_entities: Vec<NerEntity> = Vec::new();
        let model = self.primary_model.clone();
        let model_used = model.clone();

        // Build allowed types list from categories
        let (_, allowed_types) = build_ner_prompt("", &categories);
        if let Some(ref cats) = categories {
            self.log(&format!("Kategorie: {} (typy: {})", cats.join(", "), allowed_types.join(", ")));
        }

        self.log(&format!("Chunki: {}, model: {}", chunks.len(), model));

        let max_retries = 3u32;
        let mut skipped_chunks = 0usize;

        for (i, chunk) in chunks.iter().enumerate() {
            self.log(&format!("Chunk {}/{} ({} znaków) → {}", i + 1, chunks.len(), chunk.len(), model));
            let (prompt, _) = build_ner_prompt(chunk, &categories);

            let mut last_err = String::new();
            let mut success = false;

            for attempt in 0..max_retries {
                if attempt > 0 {
                    let delay = 1u64 << attempt; // 2s, 4s
                    self.log(&format!("  ⏳ Retry {}/{} za {}s...", attempt + 1, max_retries, delay));
                    tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                }

                match self.call_ollama(&prompt, &model).await {
                    Ok(response) => {
                        self.log(&format!("  Odpowiedź ({} znaków): {}",
                            response.len(), response
                        ));
                        let entities = Self::extract_json(&response);
                        self.log(&format!("  → {} encji", entities.len()));
                        all_entities.extend(entities);
                        success = true;
                        break;
                    }
                    Err(e) => {
                        last_err = e;
                    }
                }
            }

            if !success {
                skipped_chunks += 1;
                self.log(&format!("  ⚠️ Chunk {}/{} pominięty po {} próbach: {}", i + 1, chunks.len(), max_retries, last_err));
            }
        }

        if skipped_chunks > 0 {
            self.log(&format!("⚠️ Pominięto {}/{} chunków — wynik może być niepełny", skipped_chunks, chunks.len()));
        }

        let mut seen = std::collections::HashSet::new();
        all_entities.retain(|e| {
            let key = e.text.trim().to_string();
            if key.is_empty() || seen.contains(&key) {
                false
            } else {
                seen.insert(key);
                true
            }
        });

        // Regex safety net: catch patterns that small models miss
        let regex_patterns: &[(&str, &str)] = &[
            (r"\b[A-Z]{3}\d{6}\b", "OTHER_ID"),  // Polish ID card: ABC123456
        ];
        for (pattern, ner_type) in regex_patterns {
            let re = regex::Regex::new(pattern).unwrap();
            for m in re.find_iter(text) {
                let found = m.as_str().to_string();
                if !seen.contains(&found)
                    && !all_entities.iter().any(|e| e.text.contains(&found))
                {
                    self.log(&format!("  regex fallback: '{}' → {}", found, ner_type));
                    all_entities.push(NerEntity { text: found.clone(), entity_type: ner_type.to_string() });
                    seen.insert(found);
                }
            }
        }

        // Post-hoc filter: remove entities of disabled types
        // Unknown types (model-invented) are treated as ID category
        let id_types: Vec<String> = CATEGORY_TYPES.iter()
            .find(|(cat, _)| *cat == "ID")
            .map(|(_, types)| types.iter().map(|t| t.to_string()).collect())
            .unwrap_or_default();
        if !allowed_types.is_empty() {
            let id_allowed = allowed_types.iter().any(|t| id_types.contains(t));
            let before = all_entities.len();
            all_entities.retain(|e| {
                let is_known = allowed_types.iter().any(|t| t == &e.entity_type);
                let is_unknown = !CATEGORY_TYPES.iter()
                    .any(|(_, types)| types.contains(&e.entity_type.as_str()));
                is_known || (is_unknown && id_allowed)
            });
            let filtered = before - all_entities.len();
            if filtered > 0 {
                self.log(&format!("Odfiltrowano {} encji z wyłączonych kategorii", filtered));
            }
        }

        self.log(&format!("Unikalne encje: {}", all_entities.len()));

        for entity in &all_entities {
            let token = self.get_or_create_token(&entity.text, &entity.entity_type);
            let display = if entity.text.len() > 30 { &entity.text[..30] } else { &entity.text };
            self.log(&format!("  {} {} → {}", entity.entity_type, display, token));
        }

        // Single-pass replacement using Aho-Corasick (O(n) instead of O(n*m))
        let mut sorted: Vec<_> = self.entities.iter().collect();
        sorted.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

        let patterns: Vec<&str> = sorted.iter().map(|(k, _)| k.as_str()).collect();
        let replacements: Vec<&str> = sorted.iter().map(|(_, v)| v.token.as_str()).collect();

        let anon_text = if !patterns.is_empty() {
            let ac = AhoCorasick::builder()
                .match_kind(aho_corasick::MatchKind::LeftmostLongest)
                .build(&patterns)
                .unwrap();
            let result = ac.replace_all(text, &replacements);

            // Log what was replaced
            for (original, info) in &sorted {
                if result.contains(&info.token) {
                    self.log(&format!("  ✓ zamieniono '{}' → {}", original, info.token));
                } else {
                    self.log(&format!("  ✗ NIE ZNALEZIONO '{}' w tekście (token: {})", original, info.token));
                }
            }
            result
        } else {
            text.to_string()
        };

        self.last_model_used = model_used.clone();
        self.log("Zamiana tokenów zakończona");

        Ok(AnonymizeResult {
            text: anon_text,
            entities_found: all_entities.len(),
            model_used,
        })
    }

    /// Export current mapping as AnonMap JSON
    pub fn export_map(&self) -> Result<String, String> {
        let map = AnonMap {
            meta: AnonMapMeta {
                source_file: self.last_source_file.clone(),
                created: chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
                model: self.last_model_used.clone(),
                entities_count: self.entities.len(),
                thaler_version: env!("CARGO_PKG_VERSION").to_string(),
            },
            entities: self.get_mapping(),
            reverse: self.reverse.clone(),
        };

        serde_json::to_string_pretty(&map)
            .map_err(|e| format!("Błąd serializacji mapy: {}", e))
    }

    /// Deanonymize a DOCX file — replace tokens with originals in word/document.xml
    pub fn deanonymize_docx(docx_bytes: &[u8], map_json: &str) -> Result<(Vec<u8>, DeanonStats), String> {
        let map: AnonMap = serde_json::from_str(map_json)
            .map_err(|e| format!("Błąd parsowania mapy: {}", e))?;

        let mut pairs: Vec<_> = map.reverse.iter().collect();
        pairs.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

        let cursor = std::io::Cursor::new(docx_bytes);
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|e| format!("Nie mogę otworzyć DOCX: {}", e))?;

        let mut doc_xml = String::new();
        {
            use std::io::Read;
            let mut doc_file = archive.by_name("word/document.xml")
                .map_err(|e| format!("Brak word/document.xml: {}", e))?;
            doc_file.read_to_string(&mut doc_xml)
                .map_err(|e| format!("Błąd odczytu XML: {}", e))?;
        }

        // Count which tokens are present
        let total = pairs.len();
        let mut found = 0;
        let mut missing: Vec<String> = Vec::new();
        for (token, _) in &pairs {
            if doc_xml.contains(token.as_str()) {
                found += 1;
            } else {
                missing.push(token.to_string());
            }
        }
        let stats = DeanonStats { total, found, missing };

        let re_wt = regex::Regex::new(r#"(<w:t[^>]*>)(.*?)(</w:t>)"#).unwrap();
        let restored_xml = re_wt.replace_all(&doc_xml, |caps: &regex::Captures| {
            let open_tag = &caps[1];
            let mut content = caps[2].to_string();
            let close_tag = &caps[3];
            for (token, original) in &pairs {
                content = content.replace(token.as_str(), original.as_str());
            }
            format!("{}{}{}", open_tag, content, close_tag)
        }).to_string();

        // Rebuild ZIP
        let mut output_buf = Vec::new();
        {
            let w = std::io::Cursor::new(&mut output_buf);
            let mut zip_writer = zip::ZipWriter::new(w);

            for i in 0..archive.len() {
                let mut entry = archive.by_index(i)
                    .map_err(|e| format!("Błąd ZIP entry {}: {}", i, e))?;

                let options = zip::write::SimpleFileOptions::default()
                    .compression_method(entry.compression());

                let name = entry.name().to_string();
                zip_writer.start_file(&name, options)
                    .map_err(|e| format!("Błąd zapisu ZIP {}: {}", name, e))?;

                if name == "word/document.xml" {
                    use std::io::Write;
                    zip_writer.write_all(restored_xml.as_bytes())
                        .map_err(|e| format!("Błąd zapisu document.xml: {}", e))?;
                } else {
                    let buf = read_zip_entry_safe(&mut entry)?;
                    use std::io::Write;
                    zip_writer.write_all(&buf)
                        .map_err(|e| format!("Błąd zapisu {}: {}", name, e))?;
                }
            }

            zip_writer.finish()
                .map_err(|e| format!("Błąd finalizacji ZIP: {}", e))?;
        }

        Ok((output_buf, stats))
    }

    /// Export anonymized XLSX — replaces entities in xl/sharedStrings.xml inside the original ZIP
    pub fn export_anon_xlsx(&mut self, randomize_amounts: bool) -> Result<Vec<u8>, String> {
        let original_bytes = self.original_file_bytes.as_ref()
            .ok_or("Brak oryginalnego pliku XLSX w pamięci")?;

        if self.original_file_ext != "xlsx" && self.original_file_ext != "xls" {
            return Err(format!("Oryginał to .{}, nie .xlsx", self.original_file_ext));
        }

        // Build Aho-Corasick replacer (single-pass, no token corruption)
        let mut sorted: Vec<_> = self.entities.iter().collect();
        sorted.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        let patterns: Vec<&str> = sorted.iter().map(|(k, _)| k.as_str()).collect();
        let replacements: Vec<&str> = sorted.iter().map(|(_, v)| v.token.as_str()).collect();
        let ac = AhoCorasick::builder()
            .match_kind(aho_corasick::MatchKind::LeftmostLongest)
            .build(&patterns)
            .unwrap();

        if !randomize_amounts {
            return Self::replace_in_xlsx_shared_strings(original_bytes, |text| {
                ac.replace_all(text, &replacements)
            });
        }

        // --- Randomize amounts mode ---
        // Use pre-built random map from prepare_random_amounts() (called at "Anonimizuj" time).
        // Build a lookup: original_value → random_token for AMOUNT entities.
        let amount_lookup: HashMap<&str, &str> = self.entities.iter()
            .filter(|(_, info)| info.entity_type == "AMOUNT")
            .map(|(orig, info)| (orig.as_str(), info.token.as_str()))
            .collect();

        let cursor = std::io::Cursor::new(original_bytes);
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|e| format!("Nie mogę otworzyć XLSX: {}", e))?;

        let re_t = regex::Regex::new(r"(?s)(<t[^>]*>)(.*?)(</t>)").unwrap();
        let re_v = regex::Regex::new(r"(?s)(<v>)(.*?)(</v>)").unwrap();
        let re_cell_block = regex::Regex::new(r"(?s)(<c\s[^>]*>)(.*?)(</c>)").unwrap();
        // Convert self-closing <c .../> to <c ...></c> before processing
        let re_cell_selfclose = regex::Regex::new(r"<c(\s[^>]*?)\s*/>").unwrap();

        let mut modified_files: HashMap<String, Vec<u8>> = HashMap::new();

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)
                .map_err(|e| format!("Błąd ZIP entry {}: {}", i, e))?;
            let name = entry.name().to_string();

            let buf = read_zip_entry_safe(&mut entry)?;

            if name == "xl/workbook.xml" {
                // Force full recalculation on open — cached <v> in formula cells are stale
                let xml = String::from_utf8_lossy(&buf).to_string();
                let modified = regex::Regex::new(r"<calcPr([^/]*?)/>")
                    .unwrap()
                    .replace(&xml, |caps: &regex::Captures| {
                        let attrs = &caps[1];
                        if attrs.contains("fullCalcOnLoad") {
                            format!("<calcPr{}/>", attrs)
                        } else {
                            format!("<calcPr{} fullCalcOnLoad=\"1\"/>", attrs)
                        }
                    }).to_string();
                modified_files.insert(name, modified.into_bytes());
            } else if name == "xl/sharedStrings.xml" {
                // Shared strings — apply token replacement for non-amount entities
                let xml = String::from_utf8_lossy(&buf).to_string();
                let modified = re_t.replace_all(&xml, |caps: &regex::Captures| {
                    let open_tag = &caps[1];
                    let content = ac.replace_all(&caps[2], &replacements);
                    let close_tag = &caps[3];
                    format!("{}{}{}", open_tag, content, close_tag)
                }).to_string();
                modified_files.insert(name, modified.into_bytes());
            } else if name.starts_with("xl/worksheets/") && name.ends_with(".xml") {
                let xml = String::from_utf8_lossy(&buf).to_string();

                // Expand self-closing <c .../> to <c ...></c> so re_cell_block works correctly
                let xml = re_cell_selfclose.replace_all(&xml, |caps: &regex::Captures| {
                    format!("<c{}></c>", &caps[1])
                }).to_string();

                let result = re_cell_block.replace_all(&xml, |caps: &regex::Captures| {
                    let open_c = &caps[1];
                    let inner = &caps[2];
                    let close_c = &caps[3];

                    let has_formula = inner.contains("<f");
                    let is_shared_string = open_c.contains("t=\"s\"");

                    if has_formula || is_shared_string {
                        return format!("{}{}{}", open_c, inner, close_c);
                    }

                    // Non-formula, non-shared-string cell — apply random from pre-built map
                    let new_inner = re_v.replace_all(inner, |vcaps: &regex::Captures| {
                        let v_open = &vcaps[1];
                        let value = &vcaps[2];
                        let v_close = &vcaps[3];

                        // Exact match against pre-built amount lookup
                        if let Some(rand_token) = amount_lookup.get(value) {
                            return format!("{}{}{}", v_open, rand_token, v_close);
                        }

                        // Not in amount map — leave unchanged
                        format!("{}{}{}", v_open, value, v_close)
                    }).to_string();

                    // Handle inline <t> tags — token replacement for text
                    let new_inner = re_t.replace_all(&new_inner, |tcaps: &regex::Captures| {
                        let t_open = &tcaps[1];
                        let content = ac.replace_all(&tcaps[2], &replacements);
                        let t_close = &tcaps[3];
                        format!("{}{}{}", t_open, content, t_close)
                    }).to_string();

                    format!("{}{}{}", open_c, new_inner, close_c)
                }).to_string();

                // Fix cell types for any token replacements in non-randomized cells
                let step2 = Self::fix_xlsx_cell_types(&result, &re_v);
                let final_xml = Self::restore_xlsx_cell_types(&step2);

                modified_files.insert(name, final_xml.into_bytes());
            } else if name.contains("_rels/") && name.ends_with(".rels") {
                let xml = String::from_utf8_lossy(&buf).to_string();
                let re_target = regex::Regex::new(r#"Target="([^"]*)""#).unwrap();
                let modified = re_target.replace_all(&xml, |caps: &regex::Captures| {
                    let target = &caps[1];
                    let replaced = ac.replace_all(target, &replacements);
                    format!(r#"Target="{}""#, replaced)
                }).to_string();
                modified_files.insert(name, modified.into_bytes());
            } else {
                modified_files.insert(name, buf);
            }
        }

        // Rebuild ZIP
        let mut output_buf = Vec::new();
        {
            let w = std::io::Cursor::new(&mut output_buf);
            let mut zip_writer = zip::ZipWriter::new(w);

            let cursor2 = std::io::Cursor::new(original_bytes);
            let mut archive2 = zip::ZipArchive::new(cursor2)
                .map_err(|e| format!("Nie mogę otworzyć XLSX: {}", e))?;

            for i in 0..archive2.len() {
                let entry = archive2.by_index(i)
                    .map_err(|e| format!("Błąd ZIP entry {}: {}", i, e))?;

                let options = zip::write::SimpleFileOptions::default()
                    .compression_method(entry.compression());

                let name = entry.name().to_string();

                // Skip calcChain.xml — Excel rebuilds it automatically,
                // and stale entries cause "repair" warnings
                if name == "xl/calcChain.xml" {
                    continue;
                }

                zip_writer.start_file(&name, options)
                    .map_err(|e| format!("Błąd zapisu ZIP {}: {}", name, e))?;

                if let Some(data) = modified_files.get(&name) {
                    use std::io::Write;
                    zip_writer.write_all(data)
                        .map_err(|e| format!("Błąd zapisu {}: {}", name, e))?;
                }
            }

            zip_writer.finish()
                .map_err(|e| format!("Błąd finalizacji ZIP: {}", e))?;
        }

        // Entity map already updated by prepare_random_amounts()

        Ok(output_buf)
    }

    /// Deanonymize an XLSX file — replace tokens with originals
    pub fn deanonymize_xlsx(xlsx_bytes: &[u8], map_json: &str) -> Result<(Vec<u8>, DeanonStats), String> {
        let map: AnonMap = serde_json::from_str(map_json)
            .map_err(|e| format!("Błąd parsowania mapy: {}", e))?;

        let mut pairs: Vec<_> = map.reverse.iter().collect();
        pairs.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

        // Separate numeric keys (random amounts) from token keys ([TH_...])
        let numeric_pairs: HashMap<&str, &str> = pairs.iter()
            .filter(|(k, _)| k.chars().all(|c| c.is_ascii_digit()))
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        // Extract XML content from ZIP to count tokens
        let cursor = std::io::Cursor::new(xlsx_bytes);
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|e| format!("Nie mogę otworzyć XLSX: {}", e))?;

        let mut all_xml = String::new();
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).ok().filter(|e| e.name().ends_with(".xml"));
            if let Some(ref mut e) = entry {
                use std::io::Read;
                let mut buf = String::new();
                if let Err(err) = e.read_to_string(&mut buf) {
                    eprintln!("[XLSX deanon] Błąd odczytu {}: {}", e.name(), err);
                }
                all_xml.push_str(&buf);
            }
        }

        let total = pairs.len();
        let mut found = 0;
        let mut missing: Vec<String> = Vec::new();
        for (token, _) in &pairs {
            if all_xml.contains(token.as_str()) {
                found += 1;
            } else {
                missing.push(token.to_string());
            }
        }

        // If no numeric (random amount) keys, use simple bulk replace
        if numeric_pairs.is_empty() {
            let bytes = Self::replace_in_xlsx_shared_strings(xlsx_bytes, |text| {
                let mut result = text.to_string();
                for (token, original) in &pairs {
                    result = result.replace(token.as_str(), original.as_str());
                }
                result
            })?;
            return Ok((bytes, DeanonStats { total, found, missing }));
        }

        // Formula-aware deanonymization: skip formula cells for numeric replacements
        let cursor2 = std::io::Cursor::new(xlsx_bytes);
        let mut archive2 = zip::ZipArchive::new(cursor2)
            .map_err(|e| format!("Nie mogę otworzyć XLSX: {}", e))?;

        let re_t = regex::Regex::new(r"(?s)(<t[^>]*>)(.*?)(</t>)").unwrap();
        let re_v = regex::Regex::new(r"(?s)(<v>)(.*?)(</v>)").unwrap();
        let re_cell_block = regex::Regex::new(r"(?s)(<c\s[^>]*>)(.*?)(</c>)").unwrap();
        let re_cell_selfclose = regex::Regex::new(r"<c(\s[^>]*?)\s*/>").unwrap();

        let text_replacer = |text: &str| -> String {
            let mut result = text.to_string();
            for (token, original) in &pairs {
                result = result.replace(token.as_str(), original.as_str());
            }
            result
        };

        let mut modified_files: HashMap<String, Vec<u8>> = HashMap::new();

        for i in 0..archive2.len() {
            let mut entry = archive2.by_index(i)
                .map_err(|e| format!("Błąd ZIP entry {}: {}", i, e))?;
            let name = entry.name().to_string();

            let buf = read_zip_entry_safe(&mut entry)?;

            if name == "xl/workbook.xml" {
                // Force full recalculation on open
                let xml = String::from_utf8_lossy(&buf).to_string();
                let modified = regex::Regex::new(r"<calcPr([^/]*?)/>")
                    .unwrap()
                    .replace(&xml, |caps: &regex::Captures| {
                        let attrs = &caps[1];
                        if attrs.contains("fullCalcOnLoad") {
                            format!("<calcPr{}/>", attrs)
                        } else {
                            format!("<calcPr{} fullCalcOnLoad=\"1\"/>", attrs)
                        }
                    }).to_string();
                modified_files.insert(name, modified.into_bytes());
            } else if name == "xl/sharedStrings.xml" {
                let xml = String::from_utf8_lossy(&buf).to_string();
                let modified = re_t.replace_all(&xml, |caps: &regex::Captures| {
                    let open_tag = &caps[1];
                    let content = text_replacer(&caps[2]);
                    let close_tag = &caps[3];
                    format!("{}{}{}", open_tag, content, close_tag)
                }).to_string();
                modified_files.insert(name, modified.into_bytes());
            } else if name.starts_with("xl/worksheets/") && name.ends_with(".xml") {
                let xml = String::from_utf8_lossy(&buf).to_string();

                // Expand self-closing <c .../> to <c ...></c>
                let xml = re_cell_selfclose.replace_all(&xml, |caps: &regex::Captures| {
                    format!("<c{}></c>", &caps[1])
                }).to_string();

                let result = re_cell_block.replace_all(&xml, |caps: &regex::Captures| {
                    let open_c = &caps[1];
                    let inner = &caps[2];
                    let close_c = &caps[3];

                    let has_formula = inner.contains("<f");

                    if has_formula {
                        // Formula cell — skip numeric replacements, only replace text tokens
                        let new_inner = re_t.replace_all(inner, |tcaps: &regex::Captures| {
                            let t_open = &tcaps[1];
                            let content = text_replacer(&tcaps[2]);
                            let t_close = &tcaps[3];
                            format!("{}{}{}", t_open, content, t_close)
                        }).to_string();
                        return format!("{}{}{}", open_c, new_inner, close_c);
                    }

                    // Non-formula cell — replace both numeric and text tokens
                    let new_inner = re_v.replace_all(inner, |vcaps: &regex::Captures| {
                        let v_open = &vcaps[1];
                        let value = &vcaps[2];
                        let v_close = &vcaps[3];

                        // Check numeric map first (random amounts)
                        if let Some(original) = numeric_pairs.get(value) {
                            return format!("{}{}{}", v_open, original, v_close);
                        }

                        // Then text token replacement
                        let replaced = text_replacer(value);
                        format!("{}{}{}", v_open, replaced, v_close)
                    }).to_string();

                    let new_inner = re_t.replace_all(&new_inner, |tcaps: &regex::Captures| {
                        let t_open = &tcaps[1];
                        let content = text_replacer(&tcaps[2]);
                        let t_close = &tcaps[3];
                        format!("{}{}{}", t_open, content, t_close)
                    }).to_string();

                    let result = format!("{}{}{}", open_c, new_inner, close_c);
                    result
                }).to_string();

                let step2 = Self::fix_xlsx_cell_types(&result, &re_v);
                let final_xml = Self::restore_xlsx_cell_types(&step2);
                modified_files.insert(name, final_xml.into_bytes());
            } else if name.contains("_rels/") && name.ends_with(".rels") {
                let xml = String::from_utf8_lossy(&buf).to_string();
                let re_target = regex::Regex::new(r#"Target="([^"]*)""#).unwrap();
                let modified = re_target.replace_all(&xml, |caps: &regex::Captures| {
                    let target = &caps[1];
                    let replaced = text_replacer(target);
                    format!(r#"Target="{}""#, replaced)
                }).to_string();
                modified_files.insert(name, modified.into_bytes());
            } else {
                modified_files.insert(name, buf);
            }
        }

        // Rebuild ZIP
        let mut output_buf = Vec::new();
        {
            let w = std::io::Cursor::new(&mut output_buf);
            let mut zip_writer = zip::ZipWriter::new(w);

            let cursor3 = std::io::Cursor::new(xlsx_bytes);
            let mut archive3 = zip::ZipArchive::new(cursor3)
                .map_err(|e| format!("Nie mogę otworzyć XLSX: {}", e))?;

            for i in 0..archive3.len() {
                let entry = archive3.by_index(i)
                    .map_err(|e| format!("Błąd ZIP entry {}: {}", i, e))?;

                let options = zip::write::SimpleFileOptions::default()
                    .compression_method(entry.compression());

                let name = entry.name().to_string();

                // Skip calcChain.xml — Excel rebuilds it automatically
                if name == "xl/calcChain.xml" {
                    continue;
                }

                zip_writer.start_file(&name, options)
                    .map_err(|e| format!("Błąd zapisu ZIP {}: {}", name, e))?;

                if let Some(data) = modified_files.get(&name) {
                    use std::io::Write;
                    zip_writer.write_all(data)
                        .map_err(|e| format!("Błąd zapisu {}: {}", name, e))?;
                }
            }

            zip_writer.finish()
                .map_err(|e| format!("Błąd finalizacji ZIP: {}", e))?;
        }

        Ok((output_buf, DeanonStats { total, found, missing }))
    }

    /// Helper: open XLSX ZIP, apply replacer function to text content in
    /// xl/sharedStrings.xml (<t> tags) and xl/worksheets/*.xml (<v> tags for numeric cells).
    fn replace_in_xlsx_shared_strings<F>(xlsx_bytes: &[u8], replacer: F) -> Result<Vec<u8>, String>
    where F: Fn(&str) -> String
    {
        let cursor = std::io::Cursor::new(xlsx_bytes);
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|e| format!("Nie mogę otworzyć XLSX: {}", e))?;

        let re_t = regex::Regex::new(r"(<t[^>]*>)(.*?)(</t>)").unwrap();
        let re_v = regex::Regex::new(r"(<v>)(.*?)(</v>)").unwrap();

        // Collect all file contents, modifying as needed
        let mut modified_files: HashMap<String, Vec<u8>> = HashMap::new();

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)
                .map_err(|e| format!("Błąd ZIP entry {}: {}", i, e))?;
            let name = entry.name().to_string();

            let buf = read_zip_entry_safe(&mut entry)?;

            if name == "xl/workbook.xml" {
                // Force full recalculation on open
                let xml = String::from_utf8_lossy(&buf).to_string();
                let modified = regex::Regex::new(r"<calcPr([^/]*?)/>")
                    .unwrap()
                    .replace(&xml, |caps: &regex::Captures| {
                        let attrs = &caps[1];
                        if attrs.contains("fullCalcOnLoad") {
                            format!("<calcPr{}/>", attrs)
                        } else {
                            format!("<calcPr{} fullCalcOnLoad=\"1\"/>", attrs)
                        }
                    }).to_string();
                modified_files.insert(name, modified.into_bytes());
            } else if name == "xl/sharedStrings.xml" {
                // Replace in shared strings (<t> tags)
                let xml = String::from_utf8_lossy(&buf).to_string();
                let modified = re_t.replace_all(&xml, |caps: &regex::Captures| {
                    let open_tag = &caps[1];
                    let content = replacer(&caps[2]);
                    let close_tag = &caps[3];
                    format!("{}{}{}", open_tag, content, close_tag)
                }).to_string();
                modified_files.insert(name, modified.into_bytes());
            } else if name.starts_with("xl/worksheets/") && name.ends_with(".xml") {
                let xml = String::from_utf8_lossy(&buf).to_string();

                // Replace in <v> tags (numeric cells)
                let step1 = re_v.replace_all(&xml, |caps: &regex::Captures| {
                    let open_tag = &caps[1];
                    let value = &caps[2];
                    let close_tag = &caps[3];
                    let replaced = replacer(value);
                    format!("{}{}{}", open_tag, replaced, close_tag)
                }).to_string();

                // Replace in <t> tags (inline string cells — from previous anonymization)
                let step2 = re_t.replace_all(&step1, |caps: &regex::Captures| {
                    let open_tag = &caps[1];
                    let content = replacer(&caps[2]);
                    let close_tag = &caps[3];
                    format!("{}{}{}", open_tag, content, close_tag)
                }).to_string();

                // Anonimizacja: numeric <v> → inlineStr token
                let step3 = Self::fix_xlsx_cell_types(&step2, &re_v);
                // Deanonimizacja: inlineStr with numeric value → back to <v>
                let final_xml = Self::restore_xlsx_cell_types(&step3);

                modified_files.insert(name, final_xml.into_bytes());
            } else if name.contains("_rels/") && name.ends_with(".rels") {
                // Replace entity values in hyperlink targets (mailto:, http://)
                let xml = String::from_utf8_lossy(&buf).to_string();
                let re_target = regex::Regex::new(r#"Target="([^"]*)""#).unwrap();
                let modified = re_target.replace_all(&xml, |caps: &regex::Captures| {
                    let target = &caps[1];
                    let replaced = replacer(target);
                    format!(r#"Target="{}""#, replaced)
                }).to_string();
                modified_files.insert(name, modified.into_bytes());
            } else {
                modified_files.insert(name, buf);
            }
        }

        // Rebuild ZIP
        let mut output_buf = Vec::new();
        {
            let w = std::io::Cursor::new(&mut output_buf);
            let mut zip_writer = zip::ZipWriter::new(w);

            // Re-read archive for entry metadata
            let cursor2 = std::io::Cursor::new(xlsx_bytes);
            let mut archive2 = zip::ZipArchive::new(cursor2)
                .map_err(|e| format!("Nie mogę otworzyć XLSX: {}", e))?;

            for i in 0..archive2.len() {
                let entry = archive2.by_index(i)
                    .map_err(|e| format!("Błąd ZIP entry {}: {}", i, e))?;

                let options = zip::write::SimpleFileOptions::default()
                    .compression_method(entry.compression());

                let name = entry.name().to_string();

                // Skip calcChain.xml — Excel rebuilds it automatically
                if name == "xl/calcChain.xml" {
                    continue;
                }

                zip_writer.start_file(&name, options)
                    .map_err(|e| format!("Błąd zapisu ZIP {}: {}", name, e))?;

                if let Some(data) = modified_files.get(&name) {
                    use std::io::Write;
                    zip_writer.write_all(data)
                        .map_err(|e| format!("Błąd zapisu {}: {}", name, e))?;
                }
            }

            zip_writer.finish()
                .map_err(|e| format!("Błąd finalizacji ZIP: {}", e))?;
        }

        Ok(output_buf)
    }

    /// Fix XLSX cell types: when a numeric <v> value is replaced with a non-numeric token,
    /// convert the cell to an inline string so Excel can display it.
    fn fix_xlsx_cell_types(xml: &str, re_v: &regex::Regex) -> String {
        // Find cells with <v> containing non-numeric values (our tokens)
        // Pattern: <c r="B5" ...><v>[TH_NIP_001]</v></c>
        // Need to change to: <c r="B5" t="inlineStr" ...><is><t>[TH_NIP_001]</t></is></c>
        let re_cell = regex::Regex::new(
            r#"(<c\s[^>]*?)(\s*>)\s*<v>(\[TH_[^\]]+\])</v>\s*(</c>)"#
        ).unwrap();

        re_cell.replace_all(xml, |caps: &regex::Captures| {
            let mut attrs = caps[1].to_string();
            let close_bracket = &caps[2];
            let token = &caps[3];
            let close_cell = &caps[4];

            // Remove existing t="..." attribute and add t="inlineStr"
            let re_type = regex::Regex::new(r#"\st="[^"]*""#).unwrap();
            attrs = re_type.replace(&attrs, "").to_string();

            format!("{} t=\"inlineStr\"{}<is><t>{}</t></is>{}", attrs, close_bracket, token, close_cell)
        }).to_string()
    }

    /// Restore XLSX cell types: when an inlineStr cell contains a purely numeric value,
    /// convert it back to a numeric <v> cell (reverse of fix_xlsx_cell_types).
    fn restore_xlsx_cell_types(xml: &str) -> String {
        // Match: <c ... t="inlineStr" ...><is><t>12345</t></is></c>
        // where the <t> content is purely numeric (digits only)
        let re_inline = regex::Regex::new(
            r#"(<c\s[^>]*?)(\s*t="inlineStr")([^>]*>)\s*<is><t>(\d+)</t></is>\s*(</c>)"#
        ).unwrap();

        re_inline.replace_all(xml, |caps: &regex::Captures| {
            let before_type = &caps[1];
            // skip caps[2] — remove t="inlineStr"
            let after_type = &caps[3];
            let number = &caps[4];
            let close_cell = &caps[5];

            format!("{}{}<v>{}</v>{}", before_type, after_type, number, close_cell)
        }).to_string()
    }

    /// Deanonymize text using external map file content
    pub fn deanonymize_with_map(text: &str, map_json: &str) -> Result<(String, DeanonStats), String> {
        let map: AnonMap = serde_json::from_str(map_json)
            .map_err(|e| format!("Błąd parsowania mapy: {}", e))?;

        let mut result = text.to_string();
        let mut pairs: Vec<_> = map.reverse.iter().collect();
        pairs.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

        let total = pairs.len();
        let mut found = 0;
        let mut missing: Vec<String> = Vec::new();

        for (token, _original) in &pairs {
            if result.contains(token.as_str()) {
                found += 1;
                result = result.replace(token.as_str(), _original.as_str());
            } else {
                missing.push(token.to_string());
            }
        }

        Ok((result, DeanonStats { total, found, missing }))
    }

    pub fn get_mapping(&self) -> Vec<EntityInfo> {
        let mut mapping: Vec<EntityInfo> = self.entities.values().cloned().collect();
        mapping.sort_by(|a, b| a.token.cmp(&b.token));
        mapping
    }

    /// Store original file bytes for native format export
    pub fn store_original_file(&mut self, bytes: Vec<u8>, ext: &str) {
        self.original_file_bytes = Some(bytes);
        self.original_file_ext = ext.to_lowercase();
    }

    /// Get the original file extension (docx, xlsx, etc.)
    pub fn get_original_ext(&self) -> &str {
        &self.original_file_ext
    }

    /// Check if we have original file bytes for native export
    pub fn has_original_file(&self) -> bool {
        self.original_file_bytes.is_some()
    }

    /// Count entities of type AMOUNT (for logging)
    pub fn count_amount_entities(&self) -> usize {
        self.entities.values().filter(|e| e.entity_type == "AMOUNT").count()
    }

    /// Scan XLSX XML and assign random 6-digit numbers to ALL numeric cell values
    /// (non-formula, non-shared-string). Adds them to self.entities so they appear in logs and map.
    /// Returns the number of unique values randomized.
    pub fn prepare_random_amounts(&mut self) -> Result<usize, String> {
        let original_bytes = self.original_file_bytes.as_ref()
            .ok_or("Brak oryginalnych bajtów pliku")?;

        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mut used_randoms: std::collections::HashSet<u32> = std::collections::HashSet::new();
        let mut value_random_map: HashMap<String, u32> = HashMap::new();

        let cursor = std::io::Cursor::new(original_bytes);
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|e| format!("Nie mogę otworzyć XLSX: {}", e))?;

        let re_cell_block = regex::Regex::new(r"(?s)(<c\s[^>]*>)(.*?)(</c>)").unwrap();
        let re_cell_selfclose = regex::Regex::new(r"<c(\s[^>]*?)\s*/>").unwrap();
        let re_v = regex::Regex::new(r"<v>([^<]+)</v>").unwrap();

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)
                .map_err(|e| format!("Błąd ZIP entry {}: {}", i, e))?;
            let name = entry.name().to_string();

            if !(name.starts_with("xl/worksheets/") && name.ends_with(".xml")) {
                continue;
            }

            use std::io::Read;
            let mut xml = String::new();
            entry.read_to_string(&mut xml)
                .map_err(|e| format!("Błąd odczytu {}: {}", name, e))?;

            // Expand self-closing <c .../> to <c ...></c>
            let xml = re_cell_selfclose.replace_all(&xml, |caps: &regex::Captures| {
                format!("<c{}></c>", &caps[1])
            }).to_string();

            for caps in re_cell_block.captures_iter(&xml) {
                let open_c = &caps[1];
                let inner = &caps[2];

                // Skip formulas and shared strings
                if inner.contains("<f") || open_c.contains("t=\"s\"") {
                    continue;
                }

                // Extract <v> value
                if let Some(vcaps) = re_v.captures(inner) {
                    let value = &vcaps[1];

                    // Only numeric values
                    if value.parse::<f64>().is_err() {
                        continue;
                    }

                    // Skip if already mapped
                    if value_random_map.contains_key(value) {
                        continue;
                    }

                    let rand_val = loop {
                        let candidate: u32 = rng.gen_range(100000..999999);
                        if !used_randoms.contains(&candidate) {
                            break candidate;
                        }
                    };
                    used_randoms.insert(rand_val);
                    value_random_map.insert(value.to_string(), rand_val);
                }
            }
        }

        // Add to entity map
        let count = value_random_map.len();
        let valid_originals: std::collections::HashSet<String> = value_random_map.keys().cloned().collect();

        for (original_value, rand_val) in value_random_map {
            let rand_str = rand_val.to_string();

            // If NER already found this value, update its token
            if let Some(info) = self.entities.get(&original_value) {
                let old_token = info.token.clone();
                self.reverse.remove(&old_token);
                self.reverse.insert(rand_str.clone(), original_value.clone());
                if let Some(entry) = self.entities.get_mut(&original_value) {
                    entry.token = rand_str;
                }
            } else {
                // New entity — add as AMOUNT
                let entity_info = EntityInfo {
                    original: original_value.clone(),
                    token: rand_str.clone(),
                    entity_type: "AMOUNT".to_string(),
                };
                self.entities.insert(original_value.clone(), entity_info);
                self.reverse.insert(rand_str, original_value);
            }
        }

        // Remove phantom AMOUNT entities — NER found these as formula results
        // but they don't exist in any non-formula cell
        let phantoms: Vec<String> = self.entities.iter()
            .filter(|(orig, info)| {
                (info.entity_type == "AMOUNT" || info.entity_type == "KWOTA")
                    && !valid_originals.contains(orig.as_str())
            })
            .map(|(orig, _)| orig.clone())
            .collect();
        for orig in &phantoms {
            if let Some(info) = self.entities.remove(orig) {
                self.reverse.remove(&info.token);
            }
        }

        Ok(count)
    }

    /// Export anonymized DOCX — replaces entities in word/document.xml inside the original ZIP
    pub fn export_anon_docx(&self) -> Result<Vec<u8>, String> {
        let original_bytes = self.original_file_bytes.as_ref()
            .ok_or("Brak oryginalnego pliku DOCX w pamięci")?;

        if self.original_file_ext != "docx" {
            return Err(format!("Oryginał to .{}, nie .docx", self.original_file_ext));
        }

        let cursor = std::io::Cursor::new(original_bytes);
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|e| format!("Nie mogę otworzyć oryginalnego DOCX: {}", e))?;

        // Read word/document.xml
        let mut doc_xml = String::new();
        {
            use std::io::Read;
            let mut doc_file = archive.by_name("word/document.xml")
                .map_err(|e| format!("Brak word/document.xml: {}", e))?;
            doc_file.read_to_string(&mut doc_xml)
                .map_err(|e| format!("Błąd odczytu XML: {}", e))?;
        }

        // Replace entities in XML content (inside <w:t> tags)
        let anon_xml = self.replace_entities_in_xml(&doc_xml);

        // Rebuild ZIP with modified document.xml
        let mut output_buf = Vec::new();
        {
            let w = std::io::Cursor::new(&mut output_buf);
            let mut zip_writer = zip::ZipWriter::new(w);

            for i in 0..archive.len() {
                let mut entry = archive.by_index(i)
                    .map_err(|e| format!("Błąd ZIP entry {}: {}", i, e))?;

                let options = zip::write::SimpleFileOptions::default()
                    .compression_method(entry.compression());

                let name = entry.name().to_string();
                zip_writer.start_file(&name, options)
                    .map_err(|e| format!("Błąd zapisu ZIP {}: {}", name, e))?;

                if name == "word/document.xml" {
                    use std::io::Write;
                    zip_writer.write_all(anon_xml.as_bytes())
                        .map_err(|e| format!("Błąd zapisu document.xml: {}", e))?;
                } else {
                    let buf = read_zip_entry_safe(&mut entry)?;
                    use std::io::Write;
                    zip_writer.write_all(&buf)
                        .map_err(|e| format!("Błąd zapisu {}: {}", name, e))?;
                }
            }

            zip_writer.finish()
                .map_err(|e| format!("Błąd finalizacji ZIP: {}", e))?;
        }

        Ok(output_buf)
    }

    /// Replace entity originals with tokens inside XML.
    /// Strategy: for each <w:p>, collect <w:t> texts with their positions,
    /// find entities in concatenated text, then distribute the replaced text
    /// back across the original <w:t> nodes preserving their boundaries.
    fn replace_entities_in_xml(&self, xml: &str) -> String {
        // Sort entities by length descending (longest first)
        let mut sorted: Vec<_> = self.entities.iter().collect();
        sorted.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

        let re_paragraph = regex::Regex::new(r"(?s)<w:p[ >].*?</w:p>").unwrap();
        let re_wt = regex::Regex::new(r#"(<w:t[^>]*>)(.*?)(</w:t>)"#).unwrap();

        re_paragraph.replace_all(xml, |pcap: &regex::Captures| {
            let paragraph = pcap[0].to_string();

            // Collect all <w:t> text contents and positions
            let wt_caps: Vec<_> = re_wt.captures_iter(&paragraph).collect();
            if wt_caps.is_empty() {
                return paragraph;
            }

            let wt_texts: Vec<String> = wt_caps.iter()
                .map(|c| c[2].to_string())
                .collect();

            // Concatenate all text
            let full_text = wt_texts.join("");

            // Apply entity replacements on concatenated text (single-pass Aho-Corasick)
            let patterns: Vec<&str> = sorted.iter().map(|(k, _)| k.as_str()).collect();
            let replacements: Vec<&str> = sorted.iter().map(|(_, v)| v.token.as_str()).collect();
            let ac = AhoCorasick::builder()
                .match_kind(aho_corasick::MatchKind::LeftmostLongest)
                .build(&patterns)
                .unwrap();
            let replaced = ac.replace_all(&full_text, &replacements);

            // If nothing changed, return original paragraph
            if replaced == full_text {
                return paragraph;
            }

            // Distribute replaced text back to original <w:t> nodes.
            // Keep each node's length proportional to original, but adjust
            // for entity replacements.
            //
            // Simple approach: map each char in replaced text back to which
            // original <w:t> it belongs to, using character offsets.
            // Since entities may change length, we walk both strings simultaneously.
            let orig_chars: Vec<char> = full_text.chars().collect();
            let repl_chars: Vec<char> = replaced.chars().collect();

            // Build a map: for each position in full_text, which wt_index it belongs to
            let mut char_to_wt: Vec<usize> = Vec::with_capacity(orig_chars.len());
            for (i, text) in wt_texts.iter().enumerate() {
                for _ in text.chars() {
                    char_to_wt.push(i);
                }
            }

            // Walk through original and replaced text to figure out where tokens landed
            // Simpler approach: just do replacements per-node where possible,
            // and for cross-node entities, put token in first node and remove from others
            let mut new_texts = wt_texts.clone();

            // First pass: replace entities that fit within single nodes
            for (original, info) in &sorted {
                for text in &mut new_texts {
                    if text.contains(original.as_str()) {
                        *text = text.replace(original.as_str(), &info.token);
                    }
                }
            }

            // Second pass: handle cross-node entities
            let check_text = new_texts.join("");
            if check_text != replaced {
                // Some entities span multiple nodes — use concatenate-and-split approach
                // Find boundaries: each node's text length defines split points
                let node_lengths: Vec<usize> = wt_texts.iter().map(|t| t.chars().count()).collect();

                // For each entity that's still not replaced (spans nodes):
                for (original, info) in &sorted {
                    let joined = new_texts.join("");
                    if !joined.contains(original.as_str()) {
                        continue; // already replaced or not present
                    }

                    // Find which nodes the entity spans
                    let concat = new_texts.join("");
                    if let Some(pos) = concat.find(original.as_str()) {
                        let end_pos = pos + original.len();

                        // Find which nodes this spans
                        let mut offset = 0;
                        let mut start_node = 0;
                        let mut end_node = 0;
                        for (i, text) in new_texts.iter().enumerate() {
                            let node_end = offset + text.len();
                            if pos >= offset && pos < node_end {
                                start_node = i;
                            }
                            if end_pos > offset && end_pos <= node_end {
                                end_node = i;
                            }
                            offset = node_end;
                        }

                        // Replace: put everything in start_node, adjust others
                        let mut combined = String::new();
                        for i in start_node..=end_node {
                            combined.push_str(&new_texts[i]);
                        }
                        combined = combined.replace(original.as_str(), &info.token);

                        new_texts[start_node] = combined;
                        for i in (start_node + 1)..=end_node {
                            if i < new_texts.len() {
                                new_texts[i] = String::new();
                            }
                        }
                    }
                }
            }

            // Rebuild paragraph with new texts
            let mut wt_index = 0;
            re_wt.replace_all(&paragraph, |caps: &regex::Captures| {
                let open_tag = &caps[1];
                let close_tag = &caps[3];
                let new_content = if wt_index < new_texts.len() {
                    &new_texts[wt_index]
                } else {
                    ""
                };
                wt_index += 1;
                format!("{}{}{}", open_tag, new_content, close_tag)
            }).to_string()
        }).to_string()
    }

    pub fn clear(&mut self) {
        self.entities.clear();
        self.reverse.clear();
        self.counters.clear();
        self.last_model_used.clear();
        self.last_source_file.clear();
        // NOTE: original_file_bytes and original_file_ext are NOT cleared here —
        // they persist from load_file until next file is loaded or app closes
    }
}
