use super::{BaseMap, Config};

fn quoted(s: &str) -> String {
    toml::Value::String(s.to_owned()).to_string()
}

fn key(help: &dyn Fn(&str) -> String, id: &str, name: &str, value: Option<String>) -> String {
    let comment = format!("# {}\n", help(id));
    match value {
        Some(v) => format!("{comment}{name} = {v}\n"),
        None => format!("{comment}# {name} = \"\"\n"),
    }
}

fn base_maps(help: &dyn Fn(&str) -> String, maps: &[BaseMap]) -> String {
    let tables: String = maps
        .iter()
        .map(|b| {
            format!(
                "[[web.base_maps]]\nname = {}\nurl = {}\nattribution = {}\nmax_zoom = {}\n",
                quoted(&b.name),
                quoted(&b.url),
                quoted(&b.attribution),
                b.max_zoom
            )
        })
        .collect();
    format!("# {}\n{tables}", help("web_base_maps"))
}

pub(super) fn render(c: &Config, help: &dyn Fn(&str) -> String) -> String {
    let k = |id: &str, name: &str, value: Option<String>| key(help, id, name, value);
    [
        "# meshcom-status configuration file.\n".to_owned(),
        "# Written by --config-save / --config-dump: comments and layout are not kept on rewrite.\n".to_owned(),
        "\n[log]\n".to_owned(),
        k("log_level", "level", Some(quoted(&c.log.level.to_string()))),
        "\n[web]\n".to_owned(),
        k(
            "web_listen_address",
            "listen_address",
            Some(quoted(&c.web.listen_address.to_string())),
        ),
        k("web_api_prefix", "api_prefix", Some(quoted(&c.web.api_prefix))),
        base_maps(help, &c.web.base_maps),
        "\n[node]\n".to_owned(),
        k("node_host", "host", c.node.host.as_deref().map(quoted)),
        k("node_http_port", "http_port", Some(c.node.http_port.to_string())),
        k(
            "node_console_enabled",
            "console_enabled",
            Some(c.node.console_enabled.to_string()),
        ),
        k(
            "node_console_port",
            "console_port",
            Some(c.node.console_port.to_string()),
        ),
        k(
            "node_kiss_enabled",
            "kiss_enabled",
            Some(c.node.kiss_enabled.to_string()),
        ),
        k("node_kiss_port", "kiss_port", Some(c.node.kiss_port.to_string())),
        "\n[database]\n".to_owned(),
        k("database_url", "url", Some(quoted(&c.database.url))),
    ]
    .concat()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Partial, build};
    use std::path::PathBuf;

    #[test]
    fn unset_setting_is_commented_out_and_keys_have_help() {
        let c = build(PathBuf::from("a"), Partial::default()).unwrap();
        let text = render(&c, &|id| format!("help of {id}"));
        assert!(text.contains("# help of node_host\n# host = \"\"\n"), "{text}");
        assert!(text.contains("# help of node_kiss_port\nkiss_port = 8001\n"), "{text}");
        assert!(!text.contains("config_path"));
    }
}
