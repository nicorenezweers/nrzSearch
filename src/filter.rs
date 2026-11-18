// nrzSearch - der Jugendschutz-Filter.
//
// Pornografische Inhalte kommen nicht in die Trefferliste - egal, was die
// Suchmaschinen melden, und auch nicht "zufaellig" bei harmlosen Fragen.
// Vier Schichten:
//   1. Die Instanz sucht immer mit safesearch=2 ("streng").
//   2. Adressen: eine eingebaute Liste bekannter Adult-Seiten, eindeutige
//      Teile im Hostnamen ("...porn...", "xnxx"), Adult-TLDs (.xxx, .porn,
//      .sex, .adult) und optional eine eigene Sperrliste (SPERRLISTE, eine
//      Domain je Zeile oder hosts-Format - z. B. die Porn-Liste des
//      Blocklist Project).
//   3. Text: Titel, Text und Pfad werden nach Woertern in vielen Sprachen
//      bewertet (stark/mittel/schwach). Ab 3 Punkten fliegt der Treffer raus,
//      bei Bildern und Videos schon ab 2.
//   4. Die Frage selbst: ist sie eindeutig pornografisch, gibt es keine
//      Treffer, nur einen Hinweis.
//
// Nichts davon wird gespeichert oder gezaehlt (ausser der anonymen Zahl der
// herausgefilterten Treffer in der Antwort).

use serde_json::Value;
use std::collections::HashSet;
use std::sync::OnceLock;

/// Bekannte Adult-Seiten (registrierbare Domain, ohne "www.").
const DOMAENEN: &[&str] = &[
    "pornhub.com", "pornhub.org", "pornhubpremium.com", "xvideos.com", "xvideos.es", "xvideos2.com", "xvideos3.com",
    "xnxx.com", "xnxx.tv", "xhamster.com", "xhamster.desi", "xhamster2.com", "xhamsterlive.com", "redtube.com",
    "youporn.com", "tube8.com", "spankbang.com", "eporner.com", "motherless.com", "onlyfans.com", "fansly.com",
    "chaturbate.com", "myfreecams.com", "camsoda.com", "stripchat.com", "bongacams.com", "livejasmin.com",
    "cam4.com", "camwhores.tv", "flirt4free.com", "imlive.com", "streamate.com", "jerkmate.com", "brazzers.com",
    "naughtyamerica.com", "realitykings.com", "bangbros.com", "mofos.com", "digitalplayground.com", "vixen.com",
    "blacked.com", "tushy.com", "deeper.com", "evilangel.com", "kink.com", "teamskeet.com", "metart.com",
    "playboy.tv", "hustler.com", "penthouse.com", "adultfriendfinder.com", "ashleymadison.com", "fling.com",
    "literotica.com", "asstr.org", "nifty.org", "sexstories.com", "hentaihaven.xxx", "hentaihaven.com", "nhentai.net",
    "hanime.tv", "hentai2read.com", "e-hentai.org", "exhentai.org", "rule34.xxx", "rule34.paheal.net", "paheal.net",
    "gelbooru.com", "danbooru.donmai.us", "sankakucomplex.com", "rule34video.com", "f95zone.to", "porn.com",
    "porntrex.com", "pornone.com", "porndig.com", "pornhat.com", "porngo.com", "pornktube.com", "pornoxo.com",
    "hqporner.com", "daftsex.com", "beeg.com", "tnaflix.com", "empflix.com", "drtuber.com", "sunporno.com",
    "nuvid.com", "txxx.com", "upornia.com", "hclips.com", "hotmovs.com", "vjav.com", "javhd.com", "javlibrary.com",
    "missav.com", "jable.tv", "thisav.com", "porn720.com", "fapello.com", "realpornratio.com", "bellesa.co",
    "bellesa.com", "pornjoy.com", "pornmature.com", "sex3.net", "xvideo.com", "pornoente.net", "pornmove.com",
    "fapdu.com", "efukt.com", "heavy-r.com", "thothub.to", "erome.com", "imagefap.com", "pornpics.com",
    "pichunter.com", "sex.com", "xxx.com", "youjizz.com", "ixxx.com", "porzo.com", "4tube.com", "fux.com",
    "porntube.com", "pornerbros.com", "keezmovies.com", "extremetube.com", "spankwire.com", "slutload.com",
    "xbabe.com", "sxyprn.com", "anysex.com", "alphaporno.com", "pornrox.com", "fuq.com", "porn300.com",
    "yourporn.sexy", "pornwhite.com", "xozilla.com", "freeones.com", "babepedia.com", "iafd.com", "data18.com",
    "adultempire.com", "clips4sale.com", "manyvids.com", "iwantclips.com", "modelhub.com", "pornhubcasino.com",
    "redgifs.com", "gfycatporn.com", "scrolller.com", "erothots.co", "nudogram.com", "fapopedia.net",
    "celebjihad.com", "ancensored.com", "mrskin.com", "nudecelebforum.com", "coomer.su", "coomer.party",
    "kemono.su", "kemono.party", "simpcity.su", "leakedmodels.com", "thotsbay.com", "influencersgonewild.com",
    "erome.io", "nsfw.xxx", "nudostar.com", "porngames.com", "nutaku.net", "lewdzone.com", "hentaigasm.com",
    "luscious.net", "tsumino.com", "hitomi.la", "8muses.com", "multporn.net", "porncomix.info", "doujins.com",
    "xxxlist.net", "thepornlist.net", "theporndude.com", "pornmd.com", "porngeek.com", "badjojo.com",
    "fapvid.com", "tubegalore.com", "lobstertube.com", "vporn.com", "pornburst.xxx", "eroprofile.com",
    "amateurporn.me", "xcafe.com", "pussyspace.com", "pornhd.com", "pornhd8k.net", "porn00.org", "ok.xxx",
    "pornk.ru", "porno365.com", "porno365.xxx", "pornoreino.com", "pornolandia.xxx", "xvideosporno.blog.br",
    "pornodoido.com", "sexo.com", "cumlouder.com", "serviporno.com", "porn18sex.com", "pornovideoshub.com",
    "deutschepornos.xxx", "geile-sexfilme.com", "pornodeutsch.net", "sexfilme24.org", "porndeutsch.com",
    "erotik.com", "erotikchat.de", "fundorado.de", "poppen.de", "joyclub.de", "kaufmich.com", "ladies.de",
    "erotik-markt.de", "6profis.de", "markt.de/erotik", "visit-x.net", "big7.com", "amateur.tv", "mydirtyhobby.com",
    "mydirtyhobby.de", "dirtyroulette.com", "camcontacts.com", "xlovecam.com", "sexchat.de", "sexkontakte.de",
    "fickenkontakte.com", "c-date.de", "secret.de", "erotikmarkt.de", "rotlicht.de", "huren-test.de",
    "eurogirlsescort.com", "escort-ads.com", "eros.com", "tryst.link", "slixa.com", "adultsearch.com",
    "listcrawler.com", "skipthegames.com", "megapersonals.eu", "bedpage.com", "doublelist.com",
];

/// Teile eines Hostnamens, die auch mitten im Namen eindeutig sind ("bestpornsite123").
const HOST_MITTEN: &[&str] = &[
    "porn", "xnxx", "xvideo", "xhamster", "hentai", "redtube", "youporn", "sexcam", "chaturbate", "brazzers",
    "onlyfans", "nsfw", "rule34", "livesex", "sextube",
];
/// Teile, die am Anfang eines Namensteils stehen muessen ("eroticstories", aber nicht "heroticket").
const HOST_ANFANG: &[&str] = &[
    "freesex", "hotsex", "sexvid", "sexfilm", "camgirl", "fuck", "erotik", "erotic", "nudes", "fetish", "bdsm",
    "pussy", "boobs", "titten", "ficken", "xxx",
];
/// Kurze Teile nur als eigenes Wort ("jav-hd.net", "milf.club").
const HOST_WORT: &[&str] = &["jav", "milf"];

/// Teile, die trotz eines Treffers oben harmlos sind ("xxxlutz.de", "sporn-reitsport.de").
const HOST_AUSNAHMEN: &[&str] = &["xxxl", "sporn"];

/// Adult-TLDs.
const TLDS: &[&str] = &["xxx", "porn", "sex", "adult", "sexy"];

/// Woerter mit Gewicht: 3 = eindeutig, 2 = deutlich, 1 = nur im Zusammenhang.
/// Verglichen wird mit ganzen Woertern (klein, ohne Satzzeichen).
const WOERTER: &[(&str, u8)] = &[
    // Englisch
    ("porn", 3), ("porno", 3), ("pornos", 3), ("porns", 3), ("pornography", 2), ("pornographic", 3), ("pornstar", 3),
    ("pornstars", 3), ("xxx", 3), ("xnxx", 3), ("xvideos", 3), ("xhamster", 3), ("pornhub", 3), ("hentai", 3),
    ("nsfw", 3), ("nude", 2), ("nudes", 3), ("naked", 1), ("nudity", 1), ("topless", 2), ("blowjob", 3),
    ("blowjobs", 3), ("handjob", 3), ("footjob", 3), ("rimjob", 3), ("creampie", 3), ("cumshot", 3), ("cumshots", 3),
    ("cum", 1), ("gangbang", 3), ("bukkake", 3), ("deepthroat", 3), ("threesome", 2), ("milf", 3), ("milfs", 3),
    ("gilf", 3), ("dilf", 3), ("camgirl", 3), ("camgirls", 3), ("camboy", 3), ("sexcam", 3), ("livesex", 3),
    ("sexchat", 3), ("sexting", 2), ("pussy", 2), ("dick", 1), ("cock", 1), ("tits", 2), ("titties", 3),
    ("boobs", 2), ("anal", 1), ("bdsm", 3), ("fetish", 2), ("bondage", 2), ("orgasm", 2), ("orgasms", 2),
    ("masturbation", 2), ("masturbate", 2), ("masturbating", 3), ("erotic", 2), ("erotica", 3), ("escort", 1),
    ("escorts", 2), ("stripper", 1), ("striptease", 2), ("onlyfans", 3), ("fansly", 3), ("chaturbate", 3),
    ("stripchat", 3), ("bongacams", 3), ("livejasmin", 3), ("camsoda", 3), ("myfreecams", 3), ("sex", 1),
    ("sexy", 1), ("horny", 2), ("slut", 3), ("sluts", 3), ("whore", 2), ("whores", 3), ("softcore", 3),
    ("hardcore", 1), ("uncensored", 1), ("jav", 2), ("lewd", 2), ("r34", 3), ("rule34", 3), ("futanari", 3),
    ("ecchi", 2), ("yaoi", 1), ("sextape", 3), ("sexvideo", 3), ("sexvideos", 3), ("camshow", 3), ("fucking", 2),
    ("fuck", 2), ("fucked", 2), ("dildo", 3), ("dildos", 3), ("vibrator", 1), ("orgy", 3), ("orgies", 3),
    ("incest", 2), ("stepsis", 3), ("stepmom", 2), ("upskirt", 3), ("voyeur", 2), ("xxxx", 3), ("hotwife", 3),
    ("cuckold", 3), ("squirting", 3), ("twerking", 1), ("lingerie", 1), ("bikini", 1), ("busty", 2),
    ("thong", 1), ("leaked", 1), ("leaks", 1), ("fapping", 3), ("fap", 3), ("thot", 2), ("thots", 3),
    // Deutsch
    ("pornofilm", 3), ("pornofilme", 3), ("pornovideo", 3), ("pornovideos", 3), ("sexfilm", 3), ("sexfilme", 3),
    ("sexvideo", 3), ("ficken", 3), ("fick", 3), ("gefickt", 3), ("fickt", 3), ("bumsen", 3), ("möse", 3),
    ("muschi", 2), ("titten", 3), ("nackt", 1), ("nackte", 2), ("nacktbilder", 3), ("nacktbild", 3),
    ("nacktfotos", 3), ("nacktfoto", 3), ("erotik", 2), ("erotisch", 2), ("erotikfilm", 3), ("schlampe", 3),
    ("schlampen", 3), ("hure", 2), ("huren", 2), ("nutte", 3), ("nutten", 3), ("sexkontakte", 3),
    ("sextreffen", 3), ("seitensprung", 1), ("wichsen", 3), ("wichsvorlage", 3), ("geil", 1), ("geile", 1),
    ("versaut", 2), ("versaute", 2), ("domina", 2), ("fetisch", 2), ("swinger", 2), ("analsex", 3),
    ("oralsex", 3), ("schwanz", 1), ("orgasmus", 2), ("sexfilmchen", 3), ("hausfrauen", 1), ("reife", 1),
    ("sexspielzeug", 2), ("lustvoll", 1), ("vögeln", 3), ("blasen", 1), ("dreier", 1), ("strapse", 2),
    // Franzoesisch, Spanisch, Portugiesisch, Italienisch, Niederlaendisch, Polnisch, Tuerkisch
    ("salope", 3), ("salopes", 3), ("pute", 2), ("putes", 3), ("baise", 2), ("baiser", 1), ("sexe", 1),
    ("nichons", 3), ("chaudasse", 3), ("follar", 3), ("follando", 3), ("coño", 3), ("puta", 2), ("putas", 3),
    ("tetas", 2), ("desnuda", 2), ("desnudas", 2), ("culona", 3), ("foder", 3), ("fodendo", 3), ("buceta", 3),
    ("putaria", 3), ("safada", 2), ("gostosa", 1), ("pelada", 1), ("peladas", 2), ("scopare", 3), ("pompino", 3),
    ("cazzo", 2), ("tette", 2), ("troie", 2), ("neuken", 3), ("sexfilmpjes", 3), ("naakt", 1), ("ruchanie", 3),
    ("cipa", 3), ("sikiş", 3), ("sikis", 3), ("porn0", 3), ("p0rn", 3), ("pr0n", 3),
    // Russisch, Ukrainisch
    ("порно", 3), ("секс", 1), ("эротика", 3), ("голые", 2), ("шлюхи", 3), ("шлюха", 3), ("трах", 3), ("порнуха", 3),
    // Japanisch, Chinesisch (ganze Woerter selten - siehe auch ZEICHEN)
    ("エロ", 3), ("無修正", 3), ("アダルト", 2), ("色情", 3), ("无码", 3), ("成人视频", 3),
];

/// Zeichenfolgen in Schriften ohne Leerzeichen (CJK) - als Teilstring gesucht.
const ZEICHEN: &[(&str, u8)] = &[
    ("エロ動画", 3), ("無修正", 3), ("アダルト動画", 3), ("色情", 3), ("无码", 3), ("成人视频", 3), ("黄色网站", 3),
    ("av女優", 3), ("ポルノ", 3), ("야동", 3), ("성인", 1), ("포르노", 3),
];

/// Die optionale eigene Sperrliste (Datei aus SPERRLISTE), einmal geladen.
fn sperrliste() -> &'static HashSet<String> {
    static LISTE: OnceLock<HashSet<String>> = OnceLock::new();
    LISTE.get_or_init(|| {
        let mut set: HashSet<String> = DOMAENEN.iter().map(|d| d.to_string()).collect();
        if let Ok(pfad) = std::env::var("SPERRLISTE") {
            match std::fs::read_to_string(&pfad) {
                Ok(text) => {
                    let vorher = set.len();
                    for zeile in text.lines() {
                        let zeile = zeile.split('#').next().unwrap_or("").trim();
                        // hosts-Format ("0.0.0.0 example.com") oder nur die Domain.
                        let domain = zeile.split_whitespace().last().unwrap_or("");
                        let domain = domain.trim_start_matches("www.").trim_start_matches("*.").to_lowercase();
                        if domain.contains('.') && !domain.starts_with("0.0.0.0") && domain != "localhost" {
                            set.insert(domain);
                        }
                    }
                    println!("Sperrliste {pfad}: {} Domains geladen", set.len() - vorher);
                }
                Err(e) => eprintln!("Sperrliste {pfad} nicht lesbar: {e}"),
            }
        }
        set
    })
}

/// Den Filter einmal vorbereiten (Sperrliste beim Start laden, nicht bei der ersten Suche).
pub fn vorbereiten() {
    let _ = sperrliste();
}

/// Steht der Host (oder eine seiner uebergeordneten Domains) auf einer Sperrliste?
fn host_gesperrt(host: &str) -> bool {
    let host = host.trim_start_matches("www.").to_lowercase();
    let liste = sperrliste();
    // "a.b.example.com" -> "a.b.example.com", "b.example.com", "example.com"
    let mut rest = host.as_str();
    loop {
        if liste.contains(rest) {
            return true;
        }
        match rest.split_once('.') {
            Some((_, r)) if r.contains('.') => rest = r,
            _ => break,
        }
    }
    let tld = host.rsplit('.').next().unwrap_or("");
    if TLDS.contains(&tld) {
        return true;
    }
    // Eindeutige Teile im Namen (ohne die TLD), mit Ausnahmen.
    let name = host.rsplit_once('.').map_or(host.as_str(), |(n, _)| n);
    let buchstabe = |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphabetic());
    let ausnahme = |i: usize, laenge: usize| {
        HOST_AUSNAHMEN.iter().any(|a| name.match_indices(a).any(|(j, _)| j <= i && i + laenge <= j + a.len()))
    };
    let passt = |liste: &[&str], links: bool, rechts: bool| {
        liste.iter().any(|teil| {
            name.match_indices(teil).any(|(i, _)| {
                let vorher = name[..i].chars().last();
                let nachher = name[i + teil.len()..].chars().next();
                (!links || !buchstabe(vorher)) && (!rechts || !buchstabe(nachher)) && !ausnahme(i, teil.len())
            })
        })
    };
    passt(HOST_MITTEN, false, false) || passt(HOST_ANFANG, true, false) || passt(HOST_WORT, true, true)
}

/// Die Punkte eines Textes: Summe der Gewichte aller verschiedenen Treffer-Woerter.
pub fn punkte(text: &str) -> u32 {
    let klein = text.to_lowercase();
    let mut gesehen: Vec<&str> = Vec::new();
    let mut summe = 0u32;
    for wort in klein.split(|c: char| !(c.is_alphanumeric() || c == '+')) {
        if wort.is_empty() || gesehen.contains(&wort) {
            continue;
        }
        if let Some((_, g)) = WOERTER.iter().find(|(w, _)| *w == wort) {
            summe += u32::from(*g);
            gesehen.push(wort);
        }
    }
    for (z, g) in ZEICHEN {
        if klein.contains(z) {
            summe += u32::from(*g);
        }
    }
    summe
}

/// Hat der Text ein eindeutiges Wort (Gewicht 3)?
fn eindeutig(text: &str) -> bool {
    let klein = text.to_lowercase();
    klein
        .split(|c: char| !(c.is_alphanumeric() || c == '+'))
        .any(|w| WOERTER.iter().any(|(x, g)| *g == 3 && *x == w))
        || ZEICHEN.iter().any(|(z, g)| *g == 3 && klein.contains(z))
}

fn host_von(url: &str) -> String {
    reqwest::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_lowercase)).unwrap_or_default()
}

fn pfad_von(url: &str) -> String {
    reqwest::Url::parse(url).map(|u| u.path().replace(['-', '_', '/', '.'], " ")).unwrap_or_default()
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

/// Ist dieser Treffer (Rohform der Instanz: url, title, content, img_src, thumbnail_src …)
/// pornografisch? `streng` fuer Bilder und Videos (schon ab 2 Punkten).
pub fn ist_porn(t: &Value, streng: bool) -> bool {
    let url = s(t, "url");
    if host_gesperrt(&host_von(url)) {
        return true;
    }
    // Bilder/Videos liegen oft auf anderen Hosts als die Seite.
    for k in ["img_src", "thumbnail_src", "thumbnail", "iframe_src", "parsed_url"] {
        let u = s(t, k);
        if u.starts_with("http") && host_gesperrt(&host_von(u)) {
            return true;
        }
    }
    let text = format!("{} {} {} {}", s(t, "title"), s(t, "content"), pfad_von(url), s(t, "source"));
    let p = punkte(&text);
    p >= 3 || (streng && p >= 2)
}

/// Ist die Frage selbst eindeutig pornografisch? Dann gibt es keine Treffer.
pub fn frage_gesperrt(frage: &str) -> bool {
    eindeutig(frage) || punkte(frage) >= 4 || frage.split_whitespace().any(|w| w.contains('.') && host_gesperrt(w))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn t(url: &str, title: &str, content: &str) -> Value {
        json!({"url": url, "title": title, "content": content})
    }

    #[test]
    fn harmlos_bleibt() {
        for (u, ti, c) in [
            ("https://www.xxxlutz.de/moebel", "XXXLutz Möbel", "Sofas und Betten"),
            ("https://javascript.info/", "The Modern JavaScript Tutorial", "Learn JavaScript"),
            ("https://de.wikipedia.org/wiki/Sex_Pistols", "Sex Pistols - Wikipedia", "britische Punkband"),
            ("https://www.essex.ac.uk/", "University of Essex", "Study in Essex"),
            ("https://de.wikipedia.org/wiki/Analyse", "Analyse", "Eine Analyse ist eine Untersuchung"),
            ("https://www.ford.de/escort", "Ford Escort", "Der Ford Escort ist ein Auto"),
            ("https://www.nabu.de/tiere/blaumeise", "Blaumeise", "Seifenblasen und Vögel im Garten"),
            ("https://www.zdf.de/nackt-unter-wolfen", "Nackt unter Wölfen", "Film über das KZ Buchenwald"),
            ("https://www.constitution.org/", "Constitutions of the world", "constitutional texts"),
            ("https://www.heroticket.de/", "Tickets", "Konzerte"),
            ("https://ford-escort-club.de/", "Ford Escort Club", "Oldtimer"),
            ("https://sporn-reitsport.de/", "Reitsport", "Sporen"),
        ] {
            assert!(!ist_porn(&t(u, ti, c), false), "{u} faelschlich gesperrt");
        }
    }

    #[test]
    fn porn_fliegt() {
        for (u, ti, c) in [
            ("https://de.pornhub.com/view_video.php", "Video", ""),
            ("https://bestpornsite123.net/x", "Videos", ""),
            ("https://example.xxx/", "Start", ""),
            ("https://example.com/a", "Hot milf videos", "free hd"),
            ("https://example.com/b", "Geile Titten", "kostenlos"),
            ("https://example.com/c", "Kostenlose Pornofilme", ""),
            ("https://example.com/d", "nude", "busty"),
        ] {
            assert!(ist_porn(&t(u, ti, c), false), "{u} / {ti} nicht gesperrt");
        }
    }

    #[test]
    fn fragen() {
        assert!(frage_gesperrt("porno"));
        assert!(frage_gesperrt("free xxx videos"));
        assert!(!frage_gesperrt("sex pistols album"));
        assert!(!frage_gesperrt("sexualkunde unterricht"));
        assert!(!frage_gesperrt("wetter berlin"));
    }
}
