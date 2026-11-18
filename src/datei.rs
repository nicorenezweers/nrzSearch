// nrzSearch - Dateien fuer den Arbeitsbereich lesen.
//
// POST /api/datei?name=bericht.pdf mit der Datei als Koerper: zurueck kommt
// nur der Text (PDF, Word, Excel, PowerPoint, OpenDocument). Gespeichert
// wird nichts - die Datei ist nach der Antwort vergessen. Text- und
// Bilddateien liest der Browser selbst.

use std::io::{Cursor, Read};

/// So viel Text kommt hoechstens zurueck (Zeichen).
const MAX_TEXT: usize = 400_000;

/// Den Text einer Datei. Fehler sind Saetze fuer den Menschen.
pub fn lesen(name: &str, daten: &[u8]) -> Result<(String, Option<usize>), String> {
    let endung = name.rsplit('.').next().unwrap_or("").to_lowercase();
    let (text, seiten) = match endung.as_str() {
        "pdf" => pdf(daten)?,
        "docx" | "docm" | "dotx" => (office(daten, |n| n == "word/document.xml", "w:p")?, None),
        "pptx" => {
            let text = office(daten, |n| n.starts_with("ppt/slides/slide") && n.ends_with(".xml"), "a:p")?;
            let folien = text.matches("\u{c}").count();
            (text.replace('\u{c}', "\n\n--- Folie ---\n"), Some(folien))
        }
        "xlsx" | "xlsm" => (excel(daten)?, None),
        "odt" | "ods" | "odp" | "odg" => (office(daten, |n| n == "content.xml", "text:p")?, None),
        _ => return Err(format!("Dateien vom Typ .{endung} kann nrzSearch nicht lesen.")),
    };
    let text = text.trim();
    if text.is_empty() {
        return Err("In der Datei steht kein lesbarer Text (vielleicht nur gescannte Bilder).".into());
    }
    Ok((text.chars().take(MAX_TEXT).collect(), seiten))
}

fn pdf(daten: &[u8]) -> Result<(String, Option<usize>), String> {
    // pdf-extract kann bei kaputten Dateien abstuerzen - dann nur ein Fehler.
    let daten = daten.to_vec();
    let text = std::panic::catch_unwind(move || pdf_extract::extract_text_from_mem(&daten))
        .map_err(|_| "Das PDF ist beschädigt oder verschlüsselt.".to_string())?
        .map_err(|_| "Das PDF lässt sich nicht lesen (verschlüsselt oder beschädigt).".to_string())?;
    let seiten = text.matches('\u{c}').count().max(1);
    Ok((text.replace('\u{c}', "\n"), Some(seiten)))
}

/// Die Dateien in einem ZIP (Office/OpenDocument), deren Name passt - in
/// natuerlicher Reihenfolge (slide2 vor slide10), jede als Text.
fn office(daten: &[u8], passt: impl Fn(&str) -> bool, absatz: &str) -> Result<String, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(daten)).map_err(|_| "Die Datei ist kein gültiges Office-Dokument.".to_string())?;
    let mut namen: Vec<String> = namen(&zip).into_iter().filter(|n| passt(n)).collect();
    namen.sort_by_key(|n| (n.len(), n.clone()));
    let mut text = String::new();
    for name in namen {
        text += &xml_text(&eintrag(&mut zip, &name)?, absatz);
        text.push('\u{c}');
    }
    Ok(text)
}

fn namen(zip: &zip::ZipArchive<Cursor<&[u8]>>) -> Vec<String> {
    zip.file_names().filter_map(Result::ok).map(|n| n.into_owned()).collect()
}

fn eintrag(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<String, String> {
    let mut xml = String::new();
    zip.by_name(name)
        .map_err(|_| "Ein Teil der Datei fehlt.".to_string())?
        .take(50_000_000)
        .read_to_string(&mut xml)
        .map_err(|_| "Die Datei lässt sich nicht entpacken.".to_string())?;
    Ok(xml)
}

/// Text aus XML: Absaetze werden Zeilen, Tabellenzellen Tabs, alles andere
/// an Tags faellt weg.
fn xml_text(xml: &str, absatz: &str) -> String {
    let mut aus = String::with_capacity(xml.len() / 3);
    let mut rest = xml;
    while let Some(start) = rest.find('<') {
        aus.push_str(&entities(&rest[..start]));
        let Some(ende) = rest[start..].find('>') else { break };
        let tag = &rest[start + 1..start + ende];
        if tag.starts_with(&format!("/{absatz}")) || tag.starts_with("w:br") || tag.starts_with("text:line-break") {
            aus.push('\n');
        } else if tag.starts_with("/w:tc") || tag.starts_with("/table:table-cell") || tag.starts_with("w:tab") || tag.starts_with("text:tab") {
            aus.push('\t');
        }
        rest = &rest[start + ende + 1..];
    }
    aus
}

fn entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// Excel: die gemeinsamen Texte und je Blatt die Zeilen, Zellen mit Tabs.
fn excel(daten: &[u8]) -> Result<String, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(daten)).map_err(|_| "Die Datei ist keine gültige Excel-Datei.".to_string())?;
    let texte: Vec<String> = match eintrag(&mut zip, "xl/sharedStrings.xml") {
        Ok(xml) => xml.split("<si>").skip(1).map(|si| xml_text(si.split("</si>").next().unwrap_or(""), "-")).collect(),
        Err(_) => Vec::new(),
    };
    let mut blaetter: Vec<String> = namen(&zip).into_iter().filter(|n| n.starts_with("xl/worksheets/sheet") && n.ends_with(".xml")).collect();
    blaetter.sort_by_key(|n| (n.len(), n.clone()));
    let mut aus = String::new();
    for (i, blatt) in blaetter.iter().enumerate() {
        aus += &format!("--- Blatt {} ---\n", i + 1);
        let xml = eintrag(&mut zip, blatt)?;
        for zeile in xml.split("<row").skip(1) {
            let zellen: Vec<String> = zeile
                .split("<c ")
                .skip(1)
                .map(|c| {
                    let wert = c.split("<v>").nth(1).and_then(|v| v.split("</v>").next()).unwrap_or("");
                    let inline = c.split("<t>").nth(1).and_then(|v| v.split("</t>").next());
                    let kopf = c.split('>').next().unwrap_or("");
                    if kopf.contains("t=\"s\"") {
                        wert.parse::<usize>().ok().and_then(|i| texte.get(i).cloned()).unwrap_or_default()
                    } else {
                        entities(inline.unwrap_or(wert))
                    }
                })
                .collect();
            if zellen.iter().any(|z| !z.is_empty()) {
                aus += &zellen.join("\t");
                aus.push('\n');
            }
        }
    }
    Ok(aus)
}
