use std::borrow::Cow;
use std::collections::HashMap;

use axum::body::Bytes;
use rust_embed::RustEmbed;
use serde::Serialize;
use tokio::sync::OnceCell;

use super::encoding::{self, Encoding};
use crate::config::{BaseMap, WebConfig};
use crate::errors::{Error, Result};
use crate::view;

#[derive(RustEmbed)]
#[folder = "$OUT_DIR/web"]
struct Dist;

const STORED: Encoding = Encoding::Brotli;

const CONFIG_PLACEHOLDER: &str = "/*PAGE_CONFIG*/null";

pub(crate) const INDEX: &str = "index.html";

pub(crate) struct Resource {
    pub(crate) mime: String,
    pub(crate) cache_control: &'static str,
    pub(crate) compressible: bool,
    stored: Encoding,
    forms: [OnceCell<Bytes>; Encoding::COUNT],
}

impl Resource {
    fn new(path: &str, mime: String, stored: Encoding, body: Bytes) -> Resource {
        let forms: [OnceCell<Bytes>; Encoding::COUNT] = Default::default();
        // A fresh cell is empty, so this cannot fail; the result carries nothing else.
        let _ = forms[stored.index()].set(body);
        Resource {
            compressible: encoding::compressible(&mime),
            mime,
            cache_control: if path.starts_with("assets/") {
                "public, max-age=31536000, immutable"
            } else {
                "no-cache"
            },
            stored,
            forms,
        }
    }

    pub(crate) async fn body(&self, path: &str, wanted: Encoding) -> Result<(Encoding, Bytes)> {
        let encoding = if self.compressible { wanted } else { Encoding::Identity };
        let body = if encoding == self.stored {
            self.stored_form()
        } else if encoding == Encoding::Identity {
            self.computed(path, encoding, self.stored, self.stored_form()).await?
        } else {
            let identity = Box::pin(self.body(path, Encoding::Identity)).await?.1;
            self.computed(path, encoding, Encoding::Identity, identity).await?
        };
        Ok((encoding, body))
    }

    fn stored_form(&self) -> Bytes {
        self.forms[self.stored.index()].get().cloned().unwrap_or_default()
    }

    async fn computed(&self, path: &str, to: Encoding, from: Encoding, from_data: Bytes) -> Result<Bytes> {
        let cell = &self.forms[to.index()];
        let made = cell
            .get_or_try_init(|| async {
                let failed = |source| Error::Encoding {
                    path: path.to_owned(),
                    encoding: to.token(),
                    source,
                };
                let job = move || {
                    if from == Encoding::Identity {
                        encoding::encode(to, &from_data)
                    } else {
                        encoding::decode(from, &from_data)
                    }
                };
                let data = tokio::task::spawn_blocking(job)
                    .await
                    .map_err(|e| failed(std::io::Error::other(e)))?
                    .map_err(failed)?;
                tracing::debug!("{path}: {} form computed, {} bytes", to.token(), data.len());
                Ok::<Bytes, Error>(Bytes::from(data))
            })
            .await?;
        Ok(made.clone())
    }
}

pub(crate) struct Assets {
    files: HashMap<String, Resource>,
}

impl Assets {
    pub(crate) fn load(web: &WebConfig) -> Result<Assets> {
        let mut files: HashMap<String, Resource> = Dist::iter()
            .filter(|p| p != INDEX)
            .filter_map(|p| Dist::get(&p).map(|f| (p, f)))
            .map(|(p, f)| {
                let resource = Resource::new(&p, f.metadata.mimetype().to_owned(), STORED, bytes(f.data));
                (p.into_owned(), resource)
            })
            .collect();
        let page = index(web)?;
        files.insert(
            INDEX.to_owned(),
            Resource::new(
                INDEX,
                "text/html; charset=utf-8".to_owned(),
                Encoding::Identity,
                Bytes::from(page),
            ),
        );
        Ok(Assets { files })
    }

    pub(crate) fn get(&self, path: &str) -> Option<&Resource> {
        self.files.get(path)
    }
}

fn bytes(data: Cow<'static, [u8]>) -> Bytes {
    match data {
        Cow::Borrowed(b) => Bytes::from_static(b),
        Cow::Owned(v) => Bytes::from(v),
    }
}

fn embedded(path: &str) -> Result<Vec<u8>> {
    let file = Dist::get(path).ok_or_else(|| Error::Invalid {
        what: "embedded web front end",
        input: path.into(),
        reason: "missing from the build".into(),
    })?;
    encoding::decode(STORED, &file.data).map_err(|source| Error::Encoding {
        path: path.to_owned(),
        encoding: STORED.token(),
        source,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BaseMapView<'a> {
    name: &'a str,
    url: &'a str,
    attribution: &'a str,
    max_zoom: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PageConfig<'a> {
    api_prefix: &'a str,
    limits: &'a [u32],
    default_limit: u32,
    version: &'a str,
    repository: &'a str,
    base_maps: Vec<BaseMapView<'a>>,
}

fn base_map_view(b: &BaseMap) -> BaseMapView<'_> {
    BaseMapView {
        name: &b.name,
        url: &b.url,
        attribution: &b.attribution,
        max_zoom: b.max_zoom,
    }
}

pub(crate) fn render_index(template: &str, web: &WebConfig) -> Result<String> {
    let config = PageConfig {
        api_prefix: &web.api_prefix,
        limits: &view::LIMITS,
        default_limit: view::DEFAULT_LIMIT,
        version: env!("CARGO_PKG_VERSION"),
        repository: env!("CARGO_PKG_REPOSITORY"),
        base_maps: web.base_maps.iter().map(base_map_view).collect(),
    };
    let json = serde_json::to_string(&config)?.replace("</", "<\\/");
    Ok(template.replace(CONFIG_PLACEHOLDER, &json))
}

fn index(web: &WebConfig) -> Result<String> {
    render_index(&String::from_utf8_lossy(&embedded(INDEX)?), web)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn web() -> WebConfig {
        WebConfig {
            listen_address: "127.0.0.1:1".parse().unwrap(),
            api_prefix: "/api".into(),
            base_maps: vec![BaseMap {
                name: "Topo".into(),
                url: "https://t/{z}/{x}/{y}.png".into(),
                attribution: "</script><b>x</b>".into(),
                max_zoom: 17,
            }],
        }
    }

    #[test]
    fn embedded_index_has_the_placeholder() {
        let page = embedded(INDEX).expect("index.html is embedded");
        assert!(String::from_utf8_lossy(&page).contains(CONFIG_PLACEHOLDER));
        assert!(index(&web()).unwrap().contains("\"apiPrefix\":\"/api\""));
    }

    #[test]
    fn config_is_inlined_and_escaped() {
        let html = render_index("<script>const C = /*PAGE_CONFIG*/null;</script>", &web()).unwrap();
        assert!(html.contains("\"apiPrefix\":\"/api\""), "{html}");
        assert!(
            html.contains("\"limits\":[10,30,100,500,1000],\"defaultLimit\":30"),
            "{html}"
        );
        let version = format!("\"version\":\"{}\"", env!("CARGO_PKG_VERSION"));
        assert!(html.contains(&version), "{html}");
        let repository = format!("\"repository\":\"{}\"", env!("CARGO_PKG_REPOSITORY"));
        assert!(html.contains(&repository), "{html}");
        assert!(env!("CARGO_PKG_REPOSITORY").starts_with("https://"));
        assert!(
            html.contains("\"baseMaps\":[{\"name\":\"Topo\",\"url\":\"https://t/{z}/{x}/{y}.png\""),
            "{html}"
        );
        assert!(html.contains("\"maxZoom\":17"), "{html}");
        assert!(!html.contains("</script><b>"), "{html}");
        assert!(html.contains("<\\/script>"), "{html}");
    }

    #[test]
    fn assets_have_a_media_type_and_caching() {
        let assets = Assets::load(&web()).unwrap();
        let page = assets.get(INDEX).expect("the page");
        assert_eq!(
            (page.mime.as_str(), page.cache_control),
            ("text/html; charset=utf-8", "no-cache")
        );
        assert!(page.compressible);
        assert!(assets.get("nope.js").is_none());
        let script = Resource::new(
            "assets/a-1.js",
            "text/javascript".into(),
            Encoding::Identity,
            Bytes::new(),
        );
        assert_eq!(script.cache_control, "public, max-age=31536000, immutable");
    }

    #[tokio::test]
    async fn every_form_decodes_to_the_same_body_and_is_kept() {
        let text = "body { color: red }\n".repeat(200);
        let packed = Bytes::from(encoding::encode(Encoding::Brotli, text.as_bytes()).unwrap());
        let r = Resource::new("assets/a.css", "text/css".into(), Encoding::Brotli, packed.clone());
        for e in [Encoding::Identity, Encoding::Brotli, Encoding::Zstd, Encoding::Gzip] {
            let (used, body) = r.body("assets/a.css", e).await.unwrap();
            assert_eq!(used, e);
            assert_eq!(encoding::decode(e, &body).unwrap(), text.as_bytes(), "{e:?}");
            let (_, again) = r.body("assets/a.css", e).await.unwrap();
            assert_eq!(again.as_ptr(), body.as_ptr(), "{e:?} is computed once");
        }
        assert_eq!(r.body("assets/a.css", Encoding::Brotli).await.unwrap().1, packed);
    }

    #[tokio::test]
    async fn the_rendered_page_is_compressed_from_its_identity_form() {
        let r = Resource::new(
            INDEX,
            "text/html".into(),
            Encoding::Identity,
            Bytes::from("<p>hi</p>".repeat(50)),
        );
        let (_, br) = r.body(INDEX, Encoding::Brotli).await.unwrap();
        assert_eq!(
            encoding::decode(Encoding::Brotli, &br).unwrap(),
            "<p>hi</p>".repeat(50).as_bytes()
        );
    }

    #[tokio::test]
    async fn files_that_are_not_text_are_sent_as_they_are() {
        let r = Resource::new(
            "icon.png",
            "image/png".into(),
            Encoding::Identity,
            Bytes::from_static(b"PNG"),
        );
        assert_eq!(
            r.body("icon.png", Encoding::Brotli).await.unwrap(),
            (Encoding::Identity, Bytes::from_static(b"PNG"))
        );
    }

    #[tokio::test]
    async fn a_corrupt_stored_form_is_an_error() {
        let r = Resource::new(
            "assets/x.js",
            "text/javascript".into(),
            Encoding::Brotli,
            Bytes::from_static(b"nope"),
        );
        assert!(matches!(
            r.body("assets/x.js", Encoding::Gzip).await,
            Err(Error::Encoding { .. })
        ));
    }
}
