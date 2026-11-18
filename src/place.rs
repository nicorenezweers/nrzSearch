// nrzSearch - die Info-Karte: Orte, Sehenswuerdigkeiten, Personen und
// Unternehmen.
//
// Die Daten kommen aus freien Quellen:
//   - Nominatim (OpenStreetMap) : Geokodierung, Grenzen, Einwohner
//   - Wikidata                  : Fakten, Namen, Kurzbeschreibung
//   - Wikipedia                 : der Einleitungstext und die Bilder
//
// Alles in derselben Sprache, in der gesucht wurde (Standard Deutsch).

use crate::util::{eine_zeile, enc, hole, liste, normieren, s, Bremse};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::Mutex;

/// Wikipedia und Wikidata verlangen eine Angabe, wer da fragt.
const USER_AGENT: &str = concat!("nrzSearch/", env!("CARGO_PKG_VERSION"), " (self-hosted metasearch)");

/// Eine Fakten-Regel: Eigenschaft, Name, hoechste Zahl an Werten, juengster
/// Wert gewinnt, nur bei Sehenswuerdigkeiten.
type Regel = (&'static str, &'static str, usize, bool, bool);

/// Die Fakten fuer Orte - die Angaben, die bei Berlin stehen, in derselben
/// Reihenfolge, dann die fuer Staedte, Laender und Sehenswuerdigkeiten.
const FAKTEN: &[Regel] = &[
    ("P571", "Gründung, Entstehung, Erbauung", 1, true, false),
    ("P17", "Staat", 1, false, false),
    ("P6", "Leiter der Regierung oder Verwaltung", 1, true, false),
    ("P488", "Präsident", 1, false, false),
    ("P169", "Geschäftsführer", 1, true, false),
    ("P37", "Amtssprache", 3, false, false),
    ("P1082", "Einwohnerzahl", 1, true, false),
    ("P2046", "Fläche", 1, true, false),
    ("P281", "Postleitzahl", 1, false, false),
    ("P36", "Hauptstadt", 1, false, false),
    ("P30", "Kontinent", 1, false, false),
    ("P35", "Staatsoberhaupt", 1, false, false),
    ("P474", "Vorwahl", 2, false, false),
    ("P112", "Gründer", 3, false, false),
    ("P131", "Liegt in", 1, false, false),
    ("P452", "Branche", 2, false, false),
    ("P1128", "Mitarbeiter", 1, false, false),
    ("P127", "Eigentümer", 1, false, false),
    ("P749", "Mutterunternehmen", 1, false, false),
    ("P1454", "Börse", 1, false, false),
    ("P1619", "Eröffnet", 1, false, true),
    ("P84", "Architekt", 3, false, true),
    ("P149", "Baustil", 2, false, true),
    ("P1435", "Architekturstil", 2, false, true),
    ("P1705", "Künstler", 2, false, true),
    ("P88", "Verwendet von", 2, false, true),
    ("P414", "Fluss", 2, false, true),
    ("P793", "Anlass", 2, false, true),
    ("P138", "Benannt nach", 2, false, true),
    ("P276", "Spielort", 2, false, true),
    ("P2048", "Höhe", 1, false, true),
];

const FAKTEN_FIRMA: &[Regel] = &[
    ("P112", "Gründer", 3, false, false),
    ("P169", "CEO", 1, true, false),
    ("P488", "Präsident", 1, false, false),
    ("P1037", "Leitung", 1, false, false),
    ("P571", "Gründung", 1, true, false),
    ("P159", "Hauptsitz", 1, false, false),
    ("P452", "Branche", 2, false, false),
    ("P1128", "Mitarbeiter", 1, false, false),
    ("P127", "Eigentümer", 1, false, false),
    ("P749", "Mutterunternehmen", 1, false, false),
    ("P1454", "Börse", 1, false, false),
    ("P2139", "Umsatz", 1, false, false),
    ("P8340", "Bilanzsumme", 1, false, false),
    ("P17", "Land", 1, false, false),
];

const FAKTEN_PERSON: &[Regel] = &[
    ("P569", "Geburtsdatum", 1, true, false),
    ("P19", "Geburtsort", 1, false, false),
    ("P570", "Todesdatum", 1, true, false),
    ("P106", "Beruf", 4, false, false),
    ("P26", "Ehepartner", 3, false, false),
    ("P40", "Kinder", 4, false, false),
    ("P27", "Staatsangehörigkeit", 3, false, false),
    ("P69", "Ausbildung", 3, false, false),
    ("P108", "Arbeitgeber", 2, false, false),
    ("P937", "Arbeitsort", 2, false, false),
    ("P463", "Mitgliedschaft", 2, false, false),
    ("P166", "Auszeichnung", 2, false, false),
    ("P734", "Familienname", 1, false, false),
    ("P735", "Vorname", 1, false, false),
    ("P1416", "Zugehörigkeit", 2, false, false),
];

/// Die Profile in den sozialen Netzen.
const PROFILE: &[(&str, &str, &str)] = &[
    ("P2002", "X", "https://x.com/{}"),
    ("P2003", "Instagram", "https://www.instagram.com/{}"),
    ("P2013", "Facebook", "https://www.facebook.com/{}"),
    ("P2005", "Pinterest", "https://www.pinterest.com/{}"),
    ("P2006", "TikTok", "https://www.tiktok.com/@{}"),
    ("P2397", "YouTube", "https://www.youtube.com/channel/{}"),
    ("P2007", "Twitch", "https://www.twitch.tv/{}"),
    ("P2009", "YouTube", "https://www.youtube.com/user/{}"),
    ("P2126", "Tumblr", "https://{}.tumblr.com"),
];

/// Die Woerter, an denen man eine Person erkennt. Die Wikidata-Art ("Q5")
/// ist das wichtigere Merkmal - die Woerter sind der Notfall.
const PERSONEN_WOERTER: &[&str] = &[
    "spieler", "spielerin", "schauspieler", "schauspielerin", "politiker", "politikerin", "maler", "malerin",
    "sänger", "sängerin", "komponist", "komponistin", "autor", "autorin", "schriftsteller", "wissenschaftler",
    "journalist", "journalistin", "arzt", "ärztin", "ingenieur", "ingenieurin", "anwalt", "anwältin", "richter",
    "richterin", "historiker", "philosoph", "general", "admiral", "pfarrer", "bischof", "künstler", "regisseur",
    "regisseurin", "produzent", "designer", "architekt", "ökonom", "unternehmer", "unternehmerin", "musiker",
    "pianist", "lehrer", "professor", "forscher", "entdecker", "erfinder", "milliardär", "bankier", "leiter",
    "direktor", "präsident", "kanzler", "minister", "gouverneur", "bürgermeister", "sportler",
];

/// Die Woerter, an denen man ein Unternehmen erkennt.
const FIRMEN_WOERTER: &[&str] = &[
    "unternehmen", "gesellschaft", "konzern", "holding", "bank", "firma", "hersteller", "herstellerin", "verlag",
    "zeitung", "zeitschrift", "sendung", "label", "marke", "konzerngruppe", "stiftung", "werk", "werke",
    "maschinenfabrik", "spinnerei", "muttergesellschaft", "tochtergesellschaft", "beteiligung", "gruppe", "haus",
];

/// Gewerbebetrieb, Organisation, Gesellschaft, Unternehmen, Koerperschaft,
/// Technologieunternehmen, Mischkonzern, Holding, Bank, Non-Profit.
const UNTERNEHMEN_ARTEN: &[&str] =
    &["Q4830453", "Q43229", "Q783794", "Q6881511", "Q167037", "Q18388277", "Q778575", "Q219577", "Q22687", "Q163740"];

/// Karten, Logos, Fahnen und Wappen sind keine Fotos von der Person.
const KEINE_FOTOS: &[&str] = &[
    "logo", "map", "flag", "coat of arms", "wappen", "flagge", "karte", "locator", "signature", "icon", "emblem",
    "banner", "graph", "chart", "diagram", "plaque", "sketch", "drawing", "poster", "seal", "buscar", "tweet",
    "screenshot", "groundwork", "ai-generated", "montage", "collage",
];

const MONATE: [&str; 12] =
    ["Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember"];

/// Die Art einer Sehenswuerdigkeit aus dem OSM-Typ - und wie sie im
/// Untertitel heisst.
fn poi_art(typ: &str) -> Option<(&'static str, &'static str)> {
    Some(match typ {
        "tourism" | "attraction" | "viewpoint" => ("sehenswuerdigkeit", "Sehenswuerdigkeit"),
        "historic" | "monument" | "memorial" | "ruins" => ("denkmal", "Denkmal"),
        "man_made" | "building" | "amenity" | "office" => ("bauwerk", "Bauwerk"),
        "leisure" | "park" | "garden" => ("park", "Park"),
        "natural" | "peak" => ("gegend", "Landschaft"),
        "railway" => ("bahnhof", "Bahnhof"),
        "aeroway" => ("flughafen", "Flughafen"),
        "artwork" | "statue" | "fountain" => ("kunstwerk", "Kunstwerk"),
        "museum" | "gallery" => ("museum", "Museum"),
        "castle" | "palace" => ("schloss", "Schloss"),
        "church" | "cathedral" | "chapel" | "place_of_worship" => ("kirche", "Kirche"),
        "tower" => ("turm", "Turm"),
        "gate" => ("tor", "Tor"),
        "bridge" => ("bruecke", "Bruecke"),
        "zoo" => ("zoo", "Zoo"),
        "stadium" => ("stadion", "Stadion"),
        "lighthouse" => ("leuchtturm", "Leuchtturm"),
        "hotel" => ("hotel", "Hotel"),
        "restaurant" => ("restaurant", "Restaurant"),
        "university" => ("universitaet", "Universitaet"),
        "theatre" => ("theater", "Theater"),
        "library" => ("bibliothek", "Bibliothek"),
        "hospital" => ("krankenhaus", "Krankenhaus"),
        "shop" => ("laden", "Ort"),
        _ => return None,
    })
}

/// Die Orte, die als Ort gelten (Stadt, Gemeinde ...).
fn ort_art(typ: &str) -> Option<&'static str> {
    Some(match typ {
        "city" | "town" => "stadt",
        "village" | "municipality" | "commune" => "gemeinde",
        "borough" => "bezirk",
        "suburb" | "quarter" => "stadtteil",
        "hamlet" | "locality" | "isolated_dwelling" | "farm" => "ort",
        _ => return None,
    })
}

fn ist_region(typ: &str) -> bool {
    matches!(typ, "state" | "province" | "region" | "state_district")
}

pub struct Place {
    client: reqwest::Client,
    nominatim: String,
    wikidata: String,
    /// Die Wikidata-Entities, Namen, Texte und Bilder - gemerkt.
    cache: Mutex<HashMap<String, Value>>,
    /// Nominatim erlaubt hoechstens eine Anfrage pro Sekunde, Wikipedia und
    /// Wikidata wollen auch etwas Pause.
    bremse_osm: Bremse,
    bremse_wiki: Bremse,
    bremse_data: Bremse,
}

impl Place {
    pub fn neu(client: reqwest::Client, nominatim: String, wikidata: String) -> Self {
        Self {
            client,
            nominatim,
            wikidata,
            cache: Mutex::default(),
            bremse_osm: Bremse::neu(1100),
            bremse_wiki: Bremse::neu(100),
            bremse_data: Bremse::neu(200),
        }
    }

    /// JSON holen - bei einer Sperre bis zu dreimal versuchen.
    async fn json(&self, url: &str, bremse: &Bremse) -> Option<Value> {
        for versuch in 1..=3 {
            bremse.warten().await;
            if let Some(roh) = hole(self.client.get(url).header("user-agent", USER_AGENT)).await {
                return Some(roh);
            }
            if versuch < 3 {
                tokio::time::sleep(Duration::from_millis(900 * versuch)).await;
            }
        }
        eprintln!("[place] keine Antwort von {}", url.split('?').next().unwrap_or(url));
        None
    }

    /// Wie `json`, aber gemerkt (Entities, Texte und Bilder aendern sich kaum).
    async fn gemerkt(&self, schluessel: String, url: &str, bremse: &Bremse) -> Option<Value> {
        if let Some(v) = self.cache.lock().await.get(&schluessel) {
            return Some(v.clone());
        }
        let v = self.json(url, bremse).await?;
        let mut cache = self.cache.lock().await;
        // Nicht ohne Ende wachsen lassen.
        if cache.len() > 5000 {
            cache.clear();
        }
        cache.insert(schluessel, v.clone());
        Some(v)
    }

    /// Die ganze Wikidata-Entity (Angaben, Beschreibungen, Wikipedia-Links).
    async fn entity(&self, qid: &str) -> Option<Value> {
        if qid.is_empty() {
            return None;
        }
        let url = format!(
            "{}/w/api.php?action=wbgetentities&ids={}&props=claims%7Cdescriptions%7Csitelinks%7Clabels&languages=de%7Cen&redirects=yes&format=json",
            self.wikidata,
            enc(qid)
        );
        let roh = self.gemerkt(format!("entity|{qid}"), &url, &self.bremse_data).await?;
        roh.get("entities")?.get(qid).cloned()
    }

    /// Die Namen und Wikipedia-Titel der verlinkten Eintraege (Hauptstadt,
    /// Kontinent, ...), nach Sprache gemerkt.
    async fn labels(&self, ids: &[&str], sprache: &str) -> HashMap<String, (String, String)> {
        let mut map = HashMap::new();
        let mut offen = Vec::new();
        {
            let cache = self.cache.lock().await;
            for id in ids {
                match cache.get(&format!("label|{sprache}|{id}")) {
                    Some(v) => drop(map.insert(id.to_string(), (s(v, "label").into(), s(v, "wiki").into()))),
                    None if !offen.contains(id) => offen.push(*id),
                    None => {}
                }
            }
        }
        for stueck in offen.chunks(40) {
            let url = format!(
                "{}/w/api.php?action=wbgetentities&ids={}&props=labels%7Csitelinks&languages=de%7Cen&redirects=yes&format=json",
                self.wikidata,
                enc(&stueck.join("|"))
            );
            let Some(roh) = self.json(&url, &self.bremse_data).await else { continue };
            let mut cache = self.cache.lock().await;
            for (qid, ent) in roh.get("entities").and_then(Value::as_object).into_iter().flatten() {
                let label = in_sprache(ent.get("labels"), sprache);
                let wiki = wiki_titel(ent, sprache);
                cache.insert(format!("label|{sprache}|{qid}"), json!({"label": label, "wiki": wiki}));
                map.insert(qid.clone(), (label, wiki));
            }
        }
        map
    }

    /// Eine Abfrage an die Wikipedia in der Sprache der Suche.
    async fn wikipedia(&self, art: &str, titel: &str, sprache: &str, query: &str) -> Option<Value> {
        if titel.is_empty() {
            return None;
        }
        let url = format!("https://{sprache}.wikipedia.org/w/api.php?action=query&format=json&{query}&titles={}", enc(titel));
        self.gemerkt(format!("{art}|{sprache}|{titel}"), &url, &self.bremse_wiki).await
    }

    /// Der Einleitungstext aus der Wikipedia.
    async fn wikipedia_text(&self, titel: &str, sprache: &str) -> String {
        let roh = self.wikipedia("text", titel, sprache, "prop=extracts&exintro=1&explaintext=1&redirects=1").await;
        let text = seiten(roh.as_ref()).find_map(|p| p.get("extract").and_then(Value::as_str)).map(eine_zeile);
        text.unwrap_or_default()
    }

    /// Die Bilder aus dem Wikipedia-Artikel - bei Personen und Unternehmen
    /// sind das die echten Fotos.
    async fn wikipedia_bilder(&self, titel: &str, sprache: &str) -> Vec<Value> {
        let roh = self.wikipedia("bilder", titel, sprache, "prop=images&imlimit=50").await;
        let dateien: Vec<&str> = seiten(roh.as_ref())
            .flat_map(|p| liste(p, "images"))
            .map(|b| s(b, "title"))
            .filter(|d| {
                let klein = d.to_lowercase();
                !d.is_empty() && !KEINE_FOTOS.iter().any(|v| klein.contains(v))
            })
            .take(24)
            .collect();
        if dateien.is_empty() {
            return Vec::new();
        }
        // Die URLs der Bilder holen (als Vorschaubilder).
        let roh = self
            .wikipedia("bildinfo", &dateien.join("|"), sprache, "prop=imageinfo&iiprop=url%7Cmime&iiurlwidth=900")
            .await;
        seiten(roh.as_ref())
            .filter_map(|p| {
                let info = liste(p, "imageinfo").first()?;
                let voll = s(info, "url");
                let url = Some(s(info, "thumburl")).filter(|u| !u.is_empty()).unwrap_or(voll);
                let mime = s(info, "mime");
                if !url.starts_with("https://") || !(mime.is_empty() || ["image/jpeg", "image/png", "image/webp"].iter().any(|m| mime.contains(m))) {
                    return None;
                }
                let name = s(p, "title").trim_start_matches("Datei:").trim_start_matches("File:");
                Some(json!({"url": url, "voll": if voll.is_empty() { url } else { voll }, "titel": name.split('.').next()}))
            })
            .take(8)
            .collect()
    }

    // ---- Der Ort aus den OSM-Daten ---------------------------------------

    /// Den passenden Treffer aus der Nominatim-Liste holen. Wichtig: die
    /// Frage muss wirklich der Name des Ortes sein - "Berlin Hauptbahnhof"
    /// ist kein Ort.
    fn ort_waehlen<'a>(kandidaten: &'a [Value], frage: &str) -> Option<&'a Value> {
        let q = normieren(frage);
        if q.is_empty() {
            return None;
        }
        let mut best = None;
        let mut best_punkte = 0.0;
        for k in kandidaten {
            let (typ, zusatz) = (s(k, "type"), s(k, "addresstype"));
            let ist_land = zusatz == "country" || typ == "country";
            let ist_region = ist_region(zusatz);
            let ist_stadt = ort_art(zusatz).is_some();
            let ist_ort = s(k, "category") == "place" && ort_art(typ).is_some();
            let ist_poi = poi_art(typ).or(poi_art(zusatz)).is_some();
            let extra = k.get("extratags").unwrap_or(&Value::Null);
            let winzig = [typ, zusatz].iter().any(|t| matches!(*t, "hamlet" | "isolated_dwelling" | "farm" | "locality"));
            if !(ist_land || ist_region || ist_stadt || ist_ort || ist_poi)
                // Winzige Siedlungen kommen nicht in die Karte.
                || (winzig && bevoelkerung(k).is_none())
                // Nur bedeutende Orte: eine kleine Gemeinde ist keine
                // Information, sondern bloss ein Zufallstreffer. Laender,
                // Bundeslaender und Hauptstaedte sind immer wichtig.
                || ((ist_stadt || ist_ort) && !ist_poi && !ist_land && !ist_region
                    && bevoelkerung(k).unwrap_or(0) < 5_000 && extra.get("capital").is_none())
                // Sehenswuerdigkeiten nur mit Wikipedia-Artikel - ein Gasthof
                // ohne Artikel ist keine Information.
                || (ist_poi && !ist_ort && extra.get("wikidata").is_none() && extra.get("wikipedia").is_none())
            {
                continue;
            }

            let name = normieren(s(k, "name"));
            let anzeige = normieren(s(k, "display_name"));
            let mut punkte = if name == q {
                200.0
            } else if ist_poi {
                continue; // nur wenn der Name genau passt
            } else if name.starts_with(&q) && name.len() - q.len() <= 22 {
                150.0
            } else if q.split(' ').all(|w| anzeige.split(' ').any(|d| d.starts_with(w))) {
                100.0
            } else {
                continue;
            };
            // Wichtigere Orte gewinnen den Gleichstand.
            punkte += k.get("place_rank").and_then(Value::as_f64).unwrap_or(0.0) * 0.5
                + k.get("importance").and_then(Value::as_f64).unwrap_or(0.0) * 20.0
                + if ist_land { 3.0 } else { 0.0 };
            if punkte > best_punkte {
                (best, best_punkte) = (Some(k), punkte);
            }
        }
        best
    }

    /// Die Angaben zu einem Ort zusammenbauen.
    async fn ort_bauen(&self, k: &Value, sprache: &str) -> Value {
        let leer = json!({});
        let adresse = k.get("address").unwrap_or(&leer);
        let extra = k.get("extratags").unwrap_or(&leer);
        let typ = s(k, "type");
        let adresstyp = Some(s(k, "addresstype")).filter(|a| !a.is_empty()).unwrap_or(typ);
        let land = s(adresse, "country");
        let region = ["state", "province", "region"].iter().map(|f| s(adresse, f)).find(|r| !r.is_empty()).unwrap_or("");
        let stadt = ["city", "town", "village"].iter().map(|f| s(adresse, f)).find(|r| !r.is_empty()).unwrap_or("");
        let bevoelkerung = bevoelkerung(k);

        // capital= yes (staatlich), 2 (Land), 4 (Region)
        let kapital = match s(extra, "capital") {
            "yes" => "staat",
            "2" => "land",
            "4" => "region",
            _ => "",
        };
        let poi = poi_art(typ).or(poi_art(s(k, "addresstype")));
        let art = if adresstyp == "country" {
            "land"
        } else if ist_region(adresstyp) {
            "bundesland"
        } else if let Some(a) = ort_art(adresstyp).or(ort_art(typ)) {
            a
        } else {
            poi.map_or("ort", |p| p.0)
        };

        // Die Grenze nur mitnehmen, wenn sie eine Flaeche ist und nicht zu gross.
        let grenze = k
            .get("geojson")
            .filter(|g| matches!(s(g, "type"), "Polygon" | "MultiPolygon") && g.to_string().len() <= 250_000)
            .cloned()
            .unwrap_or(Value::Null);

        let wikidata = s(extra, "wikidata");
        let osm_titel = s(extra, "wikipedia").split(':').nth(1).unwrap_or("");

        // Wikipedia-Text und Wikidata-Angaben kommen gleichzeitig.
        let (entity, extract) = tokio::join!(self.entity(wikidata), self.wikipedia_text(osm_titel, sprache));
        let entity = entity.unwrap_or_default();
        let beschreibung = in_sprache(entity.get("descriptions"), sprache);
        let titel = Some(wiki_titel(&entity, sprache)).filter(|t| !t.is_empty()).unwrap_or_else(|| osm_titel.to_string());
        let extract = if extract.is_empty() && titel != osm_titel { self.wikipedia_text(&titel, sprache).await } else { extract };
        let fakten = self.fakten_bauen(&entity, sprache, FAKTEN, art, bevoelkerung, &grenze).await;

        // Der Untertitel: zuerst die Hauptstadt, dann die Wikidata-Beschreibung,
        // dann ein Satz aus den OSM-Daten.
        let untertitel = if kapital == "staat" && !land.is_empty() {
            format!("Hauptstadt von {land}")
        } else if !kapital.is_empty() && !region.is_empty() {
            format!("Hauptstadt von {region}")
        } else if !beschreibung.is_empty() && beschreibung.chars().count() <= 62 {
            beschreibung
        } else if art == "land" {
            if region.is_empty() { "Land".into() } else { format!("Land in {region}") }
        } else if land.is_empty() {
            String::new()
        } else if art == "bundesland" {
            format!("Bundesland in {land}")
        } else if art == "stadt" {
            format!("Stadt in {land}")
        } else if art != "ort" {
            let name = match art {
                "gemeinde" => "Gemeinde",
                "bezirk" => "Bezirk",
                "stadtteil" => "Stadtteil",
                _ => poi.map_or("Ort", |p| p.1),
            };
            match Some(stadt).filter(|s| !s.is_empty()).unwrap_or(region) {
                "" => format!("{name} in {land}"),
                wo => format!("{name} in {wo}, {land}"),
            }
        } else {
            format!("Ort in {land}")
        };

        let strasse = [s(adresse, "road"), s(adresse, "house_number")].iter().filter(|t| !t.is_empty()).copied().collect::<Vec<_>>().join(" ");
        let adresse_text =
            [strasse.as_str(), s(adresse, "postcode"), stadt].iter().filter(|t| !t.is_empty()).copied().collect::<Vec<_>>().join(", ");

        // Die offizielle Website: aus den OSM-Daten, sonst aus Wikidata.
        let website = [s(extra, "website"), s(extra, "contact:website")]
            .into_iter()
            .find(|w| !w.is_empty())
            .map(String::from)
            .unwrap_or_else(|| website(&entity));

        json!({
            "name": s(k, "name"),
            "art": art,
            "untertitel": untertitel,
            "anzeige": s(k, "display_name"),
            // Nominatim liefert die Koordinaten als Text ("52.5173885").
            "lat": s(k, "lat").parse::<f64>().unwrap_or(0.0),
            "lon": s(k, "lon").parse::<f64>().unwrap_or(0.0),
            "bbox": k.get("boundingbox"),
            "osmTyp": s(k, "osm_type"),
            "osmId": k.get("osm_id").and_then(Value::as_i64).unwrap_or(0),
            "wikidata": wikidata,
            "wikipedia": s(extra, "wikipedia"),
            "wikiTitel": titel,
            "wikiUrl": wiki_url(sprache, &titel),
            "wikiUrlEn": wiki_url("en", &wiki_titel_genau(&entity, "en")),
            "extract": text_kuerzen(&extract, 700),
            "fakten": fakten,
            "adresse": adresse_text,
            "land": land,
            "landCode": s(adresse, "country_code").to_lowercase(),
            "region": region,
            "stadt": stadt,
            "bevoelkerung": bevoelkerung,
            "flaecheKm2": flaeche_km2(&grenze),
            "zeitzone": s(extra, "timezone"),
            "website": website,
            // Die Profile in den sozialen Netzen (Staedte haben die auch).
            "profile": profile(&entity),
            "hauptstadt": kapital,
            "grenze": grenze,
        })
    }

    /// Die Fakten-Liste aufbauen (Werte, Namen, Einheiten).
    async fn fakten_bauen(&self, entity: &Value, sprache: &str, regeln: &[Regel], art: &str, bevoelkerung: Option<i64>, grenze: &Value) -> Vec<Value> {
        let ist_poi = !matches!(art, "ort" | "stadt" | "land" | "bundesland" | "gemeinde" | "stadtteil");

        // Die Angaben sortieren: die bevorzugten zuerst (bei "Staat" ist das
        // der heutige, danach kommen die historischen), dann der juengste
        // Wert (Bevoelkerung, Flaeche, ...).
        let sortiert: Vec<(&Regel, Vec<&Value>)> = regeln
            .iter()
            .filter(|r| ist_poi || !r.4)
            .filter_map(|r| {
                let mut l: Vec<&Value> = entity.pointer(&format!("/claims/{}", r.0))?.as_array()?.iter().collect();
                l.sort_by_key(|c| (rang_von(c), if r.3 { -zeit_von(c) } else { 0 }));
                Some((r, l)).filter(|(_, l)| !l.is_empty())
            })
            .collect();

        // Die Namen der verlinkten Eintraege in einem Rutsch holen.
        let ids: Vec<&str> = sortiert
            .iter()
            .flat_map(|(_, l)| l.iter().take(6))
            .filter_map(|c| c.pointer("/mainsnak/datavalue/value/id")?.as_str())
            .collect();
        let labels = self.labels(&ids, sprache).await;

        let mut fakten: Vec<Value> = sortiert
            .iter()
            .filter_map(|((prop, name, anzahl, ..), l)| {
                let mut texte: Vec<(String, String)> = Vec::new();
                for (text, url) in l.iter().take(6).filter_map(|c| wert_text(c, &labels, sprache)) {
                    if !texte.iter().any(|(t, _)| *t == text) {
                        texte.push((text, url));
                    }
                    if texte.len() >= *anzahl {
                        break;
                    }
                }
                // Die Einheit noch anhaengen (km², m).
                let einheit = match *prop { "P2046" => " km²", "P2048" => " m", _ => "" };
                let wert = texte.iter().map(|(t, _)| format!("{t}{einheit}")).collect::<Vec<_>>().join(", ");
                // Jeder Wert mit seinem eigenen Link (Ehepartner, Kinder ...).
                let teile: Vec<Value> = texte.iter().map(|(t, u)| json!({"text": format!("{t}{einheit}"), "url": u})).collect();
                Some(json!({"name": name, "wert": wert, "url": texte.first()?.1, "teile": teile}))
            })
            .collect();

        // Die Einwohnerzahl steht auch in den OSM-Daten - die sind meist
        // juenger als die aus Wikidata, also gewinnen sie.
        if let Some(bev) = bevoelkerung {
            match fakten.iter_mut().find(|f| f["name"] == "Einwohnerzahl") {
                Some(f) => f["wert"] = json!(bev.to_string()),
                None => fakten.insert(0, json!({"name": "Einwohnerzahl", "wert": bev.to_string(), "url": ""})),
            }
        }
        if !fakten.iter().any(|f| f["name"] == "Fläche") {
            if let Some(f) = flaeche_km2(grenze) {
                fakten.push(json!({"name": "Fläche", "wert": format!("{f} km²"), "url": ""}));
            }
        }
        fakten
    }

    // ---- Personen und Unternehmen -----------------------------------------

    /// Die Wikidata-Suche (fuer Personen und Unternehmen).
    async fn wikidata_suche(&self, name: &str, sprache: &str) -> Vec<String> {
        let url = format!(
            "{}/w/api.php?action=wbsearchentities&search={}&language={sprache}&uselang={sprache}&type=item&limit=5&format=json",
            self.wikidata,
            enc(name)
        );
        let roh = self.json(&url, &self.bremse_data).await.unwrap_or_default();
        liste(&roh, "search").iter().map(|t| s(t, "id").to_string()).filter(|id| !id.is_empty()).collect()
    }

    /// Die Hauptbedeutung der Frage: der Wikipedia-Artikel, der genau so
    /// heisst (auch mit grossen Anfangsbuchstaben). Liefert sein
    /// Wikidata-Item - oder `Mehrdeutig` bei einer Begriffsklaerung.
    async fn hauptbedeutung(&self, frage: &str, sprache: &str) -> Haupt {
        let titel = titel_varianten(frage).join("|");
        let roh = self.wikipedia("haupt", &titel, sprache, "prop=pageprops&ppprop=wikibase_item%7Cdisambiguation&redirects=1").await;
        for seite in seiten(roh.as_ref()).filter(|p| p.get("missing").is_none()) {
            let props = seite.get("pageprops").unwrap_or(&Value::Null);
            if props.get("disambiguation").is_some() {
                return Haupt::Mehrdeutig;
            }
            if let Some(qid) = props.get("wikibase_item").and_then(Value::as_str) {
                return Haupt::Eintrag(qid.to_string());
            }
        }
        Haupt::Keine
    }

    /// Die Kurzfassung der Hauptbedeutung - ein einziger Aufruf an die
    /// Wikipedia (Titel, Beschreibung, Einleitung, Bild, ob es ein Ort ist).
    /// So steht die Info gleichzeitig mit den Treffern da; der Rest (Karte,
    /// Fakten, Wetter) kommt mit /api/ort nach.
    pub async fn kurz(&self, frage: &str, sprache: &str) -> Option<Value> {
        let sprache = infosprache(sprache);
        for titel in titel_varianten(frage) {
            let schluessel = format!("kurz|{sprache}|{titel}");
            let gemerkt = self.cache.lock().await.get(&schluessel).cloned();
            let roh = match gemerkt {
                Some(v) => v,
                None => {
                    // Kein Artikel ist eine Antwort (404) - also nicht wiederholen.
                    self.bremse_wiki.warten().await;
                    let url = format!("https://{sprache}.wikipedia.org/api/rest_v1/page/summary/{}", enc(&titel.replace(' ', "_")));
                    let Some(v) = hole(self.client.get(url).header("user-agent", USER_AGENT)).await else { continue };
                    self.cache.lock().await.insert(schluessel, v.clone());
                    v
                }
            };
            if s(&roh, "type") == "disambiguation" {
                return None;
            }
            let bild = roh.pointer("/thumbnail/source").and_then(Value::as_str).unwrap_or("");
            return Some(json!({
                "name": s(&roh, "title"),
                "untertitel": s(&roh, "description"),
                "extract": text_kuerzen(s(&roh, "extract"), 700),
                "bild": bild.replace("/320px-", "/640px-"),
                "wikiUrl": roh.pointer("/content_urls/desktop/page"),
                "wikidata": s(&roh, "wikibase_item"),
                "ort": roh.get("coordinates").is_some(),
            }));
        }
        None
    }

    /// Person (false), Unternehmen (true) oder etwas anderes (None)?
    fn einordnen(entity: &Value, sprache: &str) -> Option<bool> {
        let beschreibung = in_sprache(entity.get("descriptions"), sprache).to_lowercase();
        let arten: Vec<&str> = liste(&entity["claims"], "P31").iter().filter_map(|c| c.pointer("/mainsnak/datavalue/value/id")?.as_str()).collect();
        if arten.contains(&"Q5") || PERSONEN_WOERTER.iter().any(|w| beschreibung.contains(w)) {
            Some(false)
        } else if arten.iter().any(|a| UNTERNEHMEN_ARTEN.contains(a)) || FIRMEN_WOERTER.iter().any(|w| beschreibung.contains(w)) {
            Some(true)
        } else {
            None
        }
    }

    /// Die erste Person oder - wenn keine dabei ist - das erste Unternehmen
    /// unter den Treffern. Jeder Eintrag wird nur einmal geholt - das schont
    /// die Rate von Wikidata.
    async fn person_oder_firma(&self, treffer: &[String], sprache: &str) -> Option<Value> {
        let mut firma = None;
        for qid in treffer {
            let Some(entity) = self.entity(qid).await else { continue };
            match Self::einordnen(&entity, sprache) {
                Some(false) => return Some(self.eintrag_bauen(&entity, qid, sprache, false).await),
                Some(true) if firma.is_none() => firma = Some((entity, qid)),
                _ => {}
            }
        }
        let (entity, qid) = firma?;
        Some(self.eintrag_bauen(&entity, qid, sprache, true).await)
    }

    /// Ein Begriff (ein Wort, ein Gruss, eine Sache): nur der Wikipedia-Text
    /// und das Bild - ohne Karte.
    async fn begriff_bauen(&self, entity: &Value, qid: &str, sprache: &str) -> Value {
        let titel = wiki_titel(entity, sprache);
        let extract = self.wikipedia_text(&titel, sprache).await;
        let name = Some(in_sprache(entity.get("labels"), sprache)).filter(|l| !l.is_empty()).unwrap_or_else(|| titel.clone());
        json!({
            "name": name,
            "art": "begriff",
            "untertitel": in_sprache(entity.get("descriptions"), sprache),
            "wikidata": qid,
            "wikiTitel": titel,
            "wikiUrl": wiki_url(sprache, &titel),
            "wikiUrlEn": wiki_url("en", &wiki_titel_genau(entity, "en")),
            "extract": text_kuerzen(&extract, 700),
            "fakten": [],
            "profile": profile(entity),
            "bild": commons_bild(entity, "P18", 400),
            "bilder": [],
            "website": website(entity),
        })
    }

    /// Die Angaben zu einer Person oder einem Unternehmen.
    async fn eintrag_bauen(&self, entity: &Value, qid: &str, sprache: &str, ist_firma: bool) -> Value {
        let titel = wiki_titel(entity, sprache);
        // Manche Eintraege haben keinen Namen in der gewaehlten Sprache
        // (etwa "Apple") - dann der Titel des Artikels.
        let label = Some(in_sprache(entity.get("labels"), sprache))
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| titel.split(" (").next().unwrap_or("").trim().to_string());
        let (regeln, art) = if ist_firma { (FAKTEN_FIRMA, "unternehmen") } else { (FAKTEN_PERSON, "person") };
        let (fakten, extract, mut bilder) = tokio::join!(
            self.fakten_bauen(entity, sprache, regeln, art, None, &Value::Null),
            self.wikipedia_text(&titel, sprache),
            self.wikipedia_bilder(&titel, sprache)
        );
        // Das Portraet (P18) ist das wichtigste Bild - es kommt nach vorn.
        let portraet = commons_bild(entity, "P18", 800);
        if let Some(p) = &portraet {
            bilder.insert(0, json!({"url": p, "voll": p, "titel": format!("{label} (Wikimedia Commons)")}));
        }
        json!({
            "name": label,
            "art": art,
            "untertitel": in_sprache(entity.get("descriptions"), sprache),
            "anzeige": label,
            "lat": null,
            "lon": null,
            "wikidata": qid,
            "wikiTitel": titel,
            "wikiUrl": wiki_url(sprache, &titel),
            "wikiUrlEn": wiki_url("en", &wiki_titel_genau(entity, "en")),
            "extract": text_kuerzen(&extract, 700),
            "fakten": fakten,
            "profile": profile(entity),
            "bild": if ist_firma { commons_bild(entity, "P154", 300) } else { portraet },
            "bilder": bilder,
            "website": if ist_firma { website(entity) } else { String::new() },
            "osmTyp": "",
            "osmId": 0,
            "grenze": null,
        })
    }

    // ---- Die ganze Suche ----------------------------------------------------

    /// Der Ort, die Sehenswuerdigkeit, die Person oder das Unternehmen.
    pub async fn suchen(&self, frage: &str, sprache: &str) -> Option<Value> {
        let sprache = infosprache(sprache);
        let url = format!(
            "{}/search?q={}&format=jsonv2&limit=8&addressdetails=1&extratags=1&namedetails=1&polygon_geojson=1&polygon_threshold=0.002",
            self.nominatim,
            enc(frage)
        );
        let osm = async {
            self.bremse_osm.warten().await;
            hole(self.client.get(&url).header("user-agent", USER_AGENT).header("accept-language", "de,en")).await
        };
        // Die Wikidata-Suche und die Hauptbedeutung laufen parallel zu den
        // Ortsdaten.
        let (kandidaten, treffer, haupt) =
            tokio::join!(osm, self.wikidata_suche(frage, sprache), self.hauptbedeutung(frage, sprache));
        let kandidaten = kandidaten.unwrap_or_default();
        let kandidaten = kandidaten.as_array().map_or(&[][..], Vec::as_slice);

        // Gibt es einen Artikel, der genau so heisst, ist er gemeint: "Moin"
        // ist der Gruss, nicht die Gemeinde in Tibet. Ein Ort kommt dann nur,
        // wenn er selbst dieser Artikel ist.
        // Personen und Firmen gehen vor - "Palantir Technologies" ist die
        // Firma, nicht ihr Gebaeude.
        if let Haupt::Eintrag(qid) = &haupt {
            let entity = self.entity(qid).await?;
            if let Some(ist_firma) = Self::einordnen(&entity, sprache) {
                return Some(self.eintrag_bauen(&entity, qid, sprache, ist_firma).await);
            }
            if let Some(k) = kandidaten.iter().find(|k| k.pointer("/extratags/wikidata").and_then(Value::as_str) == Some(qid)) {
                return Some(self.ort_bauen(k, sprache).await);
            }
            return Some(self.begriff_bauen(&entity, qid, sprache).await);
        }

        // Ohne eindeutigen Artikel: ein bedeutender Ort (Land, Region,
        // Hauptstadt, Stadt ab 100 000 Einwohnern), sonst die Person oder
        // das Unternehmen aus Wikidata.
        let k = Self::ort_waehlen(kandidaten, frage).filter(|k| {
            matches!(s(k, "category"), "place" | "boundary")
                && (s(k, "addresstype") == "country"
                    || ist_region(s(k, "addresstype"))
                    || k.pointer("/extratags/capital").is_some()
                    || bevoelkerung(k).unwrap_or(0) >= 100_000)
        });
        if let Some(k) = k {
            return Some(self.ort_bauen(k, sprache).await);
        }
        self.person_oder_firma(&treffer, sprache).await
    }
}

/// Die Info-Karte gibt es auf Deutsch, Englisch, Franzoesisch,
/// Niederlaendisch, Italienisch und Spanisch - sonst Deutsch.
/// Die Wikipedia-Sprache fuer die Info-Karte: jede der 25 Sprachen, sonst Deutsch.
fn infosprache(sprache: &str) -> &str {
    let basis = sprache.split('-').next().unwrap_or("");
    crate::search::SPRACHEN.iter().map(|(s, _)| *s).find(|s| *s == basis).unwrap_or("de")
}

/// Die Frage, wie getippt und mit grossen Anfangsbuchstaben ("elon musk"
/// -> "Elon Musk") - Wikipedia-Titel unterscheiden das.
fn titel_varianten(frage: &str) -> Vec<String> {
    let gross: Vec<String> = frage
        .split_whitespace()
        .map(|w| w.chars().take(1).flat_map(char::to_uppercase).chain(w.chars().skip(1)).collect())
        .collect();
    let mut varianten = vec![frage.trim().to_string(), gross.join(" ")];
    varianten.dedup();
    varianten
}

/// Die Hauptbedeutung eines Wortes in der Wikipedia.
enum Haupt {
    Eintrag(String),
    Mehrdeutig,
    Keine,
}

/// Die Seiten einer Wikipedia-Antwort.
fn seiten(roh: Option<&Value>) -> impl Iterator<Item = &Value> {
    roh.and_then(|r| r.pointer("/query/pages")?.as_object()).into_iter().flat_map(|p| p.values())
}

/// Der Wert in der gewaehlten Sprache, sonst Deutsch.
fn in_sprache(obj: Option<&Value>, sprache: &str) -> String {
    obj.and_then(|o| o.get(sprache).or(o.get("de")))
        .map(|v| s(v, "value").to_string())
        .unwrap_or_default()
}

/// Der Titel des Wikipedia-Artikels in genau dieser Sprache.
fn wiki_titel_genau(entity: &Value, sprache: &str) -> String {
    entity.pointer(&format!("/sitelinks/{sprache}wiki/title")).and_then(Value::as_str).unwrap_or("").to_string()
}

/// Der Titel des Wikipedia-Artikels in der Sprache, sonst Deutsch.
fn wiki_titel(entity: &Value, sprache: &str) -> String {
    Some(wiki_titel_genau(entity, sprache)).filter(|t| !t.is_empty()).unwrap_or_else(|| wiki_titel_genau(entity, "de"))
}

fn wiki_url(sprache: &str, titel: &str) -> String {
    if titel.is_empty() { String::new() } else { format!("https://{sprache}.wikipedia.org/wiki/{}", titel.replace(' ', "_")) }
}

/// Die offizielle Website aus Wikidata (P856).
fn website(entity: &Value) -> String {
    entity.pointer("/claims/P856/0/mainsnak/datavalue/value").and_then(Value::as_str).unwrap_or("").to_string()
}

/// Die Profile in den sozialen Netzen.
fn profile(entity: &Value) -> Vec<Value> {
    let mut profile: Vec<Value> = Vec::new();
    for (prop, netz, muster) in PROFILE {
        if profile.iter().any(|p| p["netz"] == *netz) {
            continue;
        }
        let wert = liste(&entity["claims"], prop).iter().take(2).find_map(|c| c.pointer("/mainsnak/datavalue/value")?.as_str());
        if let Some(wert) = wert {
            profile.push(json!({"netz": netz, "url": muster.replace("{}", wert), "name": wert}));
        }
    }
    profile
}

/// Ein Text an einem Satzende abschneiden (nicht mitten im Satz).
fn text_kuerzen(text: &str, max: usize) -> String {
    let t = text.trim();
    if t.chars().count() <= max {
        return t.to_string();
    }
    let teil: String = t.chars().take(max).collect();
    match teil.rfind(". ").or_else(|| teil.rfind("! ")).or_else(|| teil.rfind("? ")) {
        Some(p) if p > 120 => teil[..=p].to_string(),
        _ => format!("{teil} …"),
    }
}

/// Die Einwohnerzahl aus den OSM-Zusatzdaten.
fn bevoelkerung(k: &Value) -> Option<i64> {
    let roh = k.pointer("/extratags/population")?.as_str()?;
    roh.chars().filter(char::is_ascii_digit).collect::<String>().parse().ok().filter(|z| *z > 0)
}

/// Der Rang einer Wikidata-Angabe: bevorzugt (0), normal (1), veraltet (2).
fn rang_von(claim: &Value) -> u8 {
    match s(claim, "rank") {
        "preferred" => 0,
        "deprecated" => 2,
        _ => 1,
    }
}

/// Das Jahr einer Wikidata-Zeit ("+2023-01-01T00:00:00Z" -> 2023).
fn jahr(zeit: &str) -> i64 {
    let ziffern: String = zeit.trim_start_matches(['+', '-']).chars().take_while(char::is_ascii_digit).collect();
    let jahr = ziffern.parse().unwrap_or(0);
    if zeit.starts_with('-') { -jahr } else { jahr }
}

/// Das Jahr einer Wikidata-Angabe ("point in time", P585).
fn zeit_von(claim: &Value) -> i64 {
    liste(claim.get("qualifiers").unwrap_or(&Value::Null), "P585")
        .iter()
        .find_map(|q| q.pointer("/datavalue/value/time")?.as_str())
        .map_or(0, jahr)
}

/// Ein Wikidata-Wert als Text (und Link).
fn wert_text(claim: &Value, labels: &HashMap<String, (String, String)>, sprache: &str) -> Option<(String, String)> {
    let wert = claim.pointer("/mainsnak/datavalue/value")?;
    let text = match wert {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ if wert.get("id").is_some() => {
            let id = s(wert, "id");
            // Ohne Namen in der gewaehlten Sprache lieber weglassen als die
            // Q-Nummer zu zeigen.
            let (label, wiki) = labels.get(id).filter(|(l, _)| !l.is_empty())?;
            let url = if wiki.is_empty() { format!("https://www.wikidata.org/wiki/{id}") } else { wiki_url(sprache, wiki) };
            return Some((label.clone(), url));
        }
        _ if wert.get("time").is_some() => {
            // Das Format ist "+JJJJ-MM-TT" - der Monat steht an Stelle 6,
            // der Tag an Stelle 9.
            let zeit = s(wert, "time");
            let jahr = jahr(zeit);
            let monat = MONATE.get(zeit.get(6..8).and_then(|m| m.parse::<usize>().ok()).unwrap_or(1).wrapping_sub(1)).unwrap_or(&"");
            let tag: u32 = zeit.get(9..11).and_then(|t| t.parse().ok()).unwrap_or(1);
            match wert.get("precision").and_then(Value::as_i64).unwrap_or(11) {
                ..=9 => jahr.to_string(),
                10 => format!("{monat} {jahr}"),
                _ => format!("{tag}. {monat} {jahr}"),
            }
        }
        _ if wert.get("amount").is_some() => {
            let zahl: f64 = s(wert, "amount").trim_start_matches('+').parse().ok().filter(|z: &f64| z.is_finite())?;
            zahl.to_string()
        }
        _ => s(wert, "text").to_string(),
    };
    Some((text, String::new())).filter(|(t, _)| !t.is_empty() || wert.is_string())
}

/// Das Bild aus Wikimedia Commons (P18 oder P154).
fn commons_bild(entity: &Value, prop: &str, breite: u32) -> Option<String> {
    let datei = entity.pointer(&format!("/claims/{prop}/0/mainsnak/datavalue/value"))?.as_str().filter(|d| !d.is_empty())?;
    Some(format!("https://commons.wikimedia.org/wiki/Special:FilePath/{}?width={breite}", enc(datei)))
}

/// Die Flaeche eines GeoJSON (Polygon oder MultiPolygon) in km².
fn flaeche_km2(geojson: &Value) -> Option<f64> {
    let polys: Vec<&Value> = match s(geojson, "type") {
        "MultiPolygon" => liste(geojson, "coordinates").iter().collect(),
        "Polygon" => vec![geojson.get("coordinates")?],
        _ => return None,
    };
    let flaeche: f64 = polys
        .iter()
        .filter_map(|p| p.as_array())
        .flat_map(|ringe| ringe.iter().enumerate().map(|(i, r)| if i == 0 { ring_flaeche(r) } else { -ring_flaeche(r) }))
        .sum();
    (flaeche > 0.0).then(|| (flaeche * 10.0).round() / 10.0)
}

/// Die Flaeche eines GeoJSON-Rings auf der Erdkugel (Radius 6371 km).
fn ring_flaeche(ring: &Value) -> f64 {
    let punkte: Vec<(f64, f64)> = ring
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| Some((p.get(0)?.as_f64()?.to_radians(), p.get(1)?.as_f64()?.to_radians())))
        .collect();
    if punkte.len() < 3 {
        return 0.0;
    }
    let summe: f64 = punkte
        .iter()
        .zip(punkte.iter().cycle().skip(1))
        .map(|((lon1, lat1), (lon2, lat2))| (lon2 - lon1) * (2.0 + lat1.sin() + lat2.sin()))
        .sum();
    (summe * 6371.0 * 6371.0 / 2.0).abs()
}
