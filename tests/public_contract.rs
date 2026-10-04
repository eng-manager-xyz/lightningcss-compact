use lightningcss_compact::{
    apply_plan, compile_project, prepare_project, BindingInput, BindingKind, Mode, Options,
    ProjectInput, StylesheetInput,
};

fn project(css: &str, html: &str, classes: &[&str]) -> ProjectInput {
    ProjectInput {
        stylesheets: vec![StylesheetInput {
            id: "fixture.css".into(),
            source: css.into(),
        }],
        bindings: vec![BindingInput {
            id: "document".into(),
            kind: BindingKind::Html,
            value: html.into(),
        }],
        complete_usage: true,
        managed_classes: classes.iter().map(|class| (*class).into()).collect(),
        load_groups: vec![vec!["fixture.css".into()]],
        ..Default::default()
    }
}

#[test]
fn complete_unobserved_styles_share_one_multi_property_class() {
    let input = project(
        ".left{color:red;height:100px}.right{color:red;height:100px}",
        "<div class=\"left\">left</div><div class=\"right\">right</div>",
        &["left", "right"],
    );
    let result = compile_project(input, Options::default()).expect("compile complete inventory");
    assert_eq!(
        result.manifest.classes["left"],
        result.manifest.classes["right"]
    );
    assert_eq!(result.manifest.classes["left"].len(), 1);
    assert!(result.stylesheets["fixture.css"].contains("color:red"));
    assert!(result.stylesheets["fixture.css"].contains("height:100px"));
    assert!(result.report.optimized.total.brotli <= result.report.baseline.total.brotli);
    assert!(result.report.optimized.total.gzip <= result.report.baseline.total.gzip);
    assert!(result
        .report
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.reason.contains("exhaustive finite search")));
}

#[test]
fn source_maps_preserve_authored_source_and_have_real_mappings() {
    let source = "/* authored source */\n.left { color: red; height: 100px; }\n.right { color: red; height: 100px; }\n";
    let input = project(
        source,
        "<div class=\"left\">left</div><div class=\"right\">right</div>",
        &["left", "right"],
    );
    let result = compile_project(input, Options::default()).expect("mapped compilation");
    let map: serde_json::Value =
        serde_json::from_str(&result.source_maps["fixture.css"]).expect("valid Source Map JSON");
    assert_eq!(map["version"], 3);
    assert_eq!(map["sources"][0], "fixture.css");
    assert_eq!(map["sourcesContent"][0], source);
    assert!(map["mappings"]
        .as_str()
        .is_some_and(|value| !value.is_empty()));
}

#[test]
fn mutable_token_bindings_reject_ascii_whitespace() {
    for value in ["", "left right", " left", "left ", "left\n", "\tleft"] {
        let mut input = project(
            ".left{color:red}",
            "<div class=\"left\">left</div>",
            &["left"],
        );
        input.bindings.push(BindingInput {
            id: "dynamic".into(),
            kind: BindingKind::Token,
            value: value.into(),
        });
        assert!(
            compile_project(input, Options::default()).is_err(),
            "accepted invalid DOMTokenList token {value:?}"
        );
    }
}

#[test]
fn empty_class_substring_observations_never_match_or_block_naming() {
    let input = project(
        ".left{color:red;height:100px}.right{color:red;height:100px}[class^=\"\"]{color:blue}[class*=\"\"]{display:none}[class$=\"\"]{height:20px}",
        "<div class=\"left\">left</div><div class=\"right\">right</div>",
        &["left", "right"],
    );
    let result =
        compile_project(input, Options::default()).expect("empty patterns preserve naming");
    assert_eq!(
        result.manifest.classes["left"],
        result.manifest.classes["right"]
    );
    assert_eq!(result.manifest.classes["left"].len(), 1);
    assert!(!result.bindings["document"].contains("class=\"left\""));
}

#[test]
fn class_string_observers_preserve_unmatched_boundary_whitespace() {
    let owner = "barReallyLongManagedClass";
    let html = format!(
        "<div class=\"{owner} foo \">trailing</div><div class=\" foo {owner}\">leading</div>"
    )
    .repeat(10);
    let input = project(
        ".barReallyLongManagedClass{height:100px}[class$=\"foo\"]{color:red}[class^=\"foo\"]{border:5px solid purple}",
        &html,
        &[owner],
    );
    let result = compile_project(input, Options::default()).expect("preserved class strings");
    let values: Vec<_> = result.bindings["document"]
        .split("class=\"")
        .skip(1)
        .map(|part| part.split('"').next().expect("attribute terminator"))
        .collect();
    assert_eq!(values.len(), 20);
    for (index, value) in values.iter().enumerate() {
        if index % 2 == 0 {
            assert!(value.ends_with(' '), "trailing boundary removed: {value:?}");
            assert!(!value.ends_with("foo"));
        } else {
            assert!(
                value.starts_with(' '),
                "leading boundary removed: {value:?}"
            );
            assert!(!value.starts_with("foo"));
        }
    }
}

#[test]
fn noscript_fallback_classes_keep_identity_and_raw_source() {
    let fallback = "<noscript><div class=\"left\">Fallback &amp; literal markup.</div><div class=\"a\">Foreign fallback.</div></noscript>";
    let html = format!("<div class=\"left\">left</div><div class=\"right\">right</div>{fallback}");
    let input = project(
        ".left{color:red;height:100px}.right{color:red;height:100px}",
        &html,
        &["left", "right"],
    );
    let result = compile_project(input, Options::default()).expect("opaque fallback inventory");
    assert!(result.bindings["document"].contains(fallback));
    assert_eq!(result.manifest.classes["left"], vec!["left"]);
    assert_eq!(result.manifest.classes["a"], vec!["a"]);
    assert!(!result.manifest.classes["right"]
        .iter()
        .any(|name| name == "a"));
    assert!(result.stylesheets["fixture.css"].contains(".left"));
}

#[test]
fn selector_preludes_and_end_boundaries_are_part_of_the_inventory() {
    let classes = lightningcss_compact::discover_classes(
        "@scope (.frame) to (.stop){.inside{color:red}}@supports selector(.feature){.enabled{color:green}}",
    )
    .expect("selector-context inventory");
    for class in ["frame", "stop", "inside", "feature", "enabled"] {
        assert!(
            classes.contains(class),
            "missing selector-context class {class}"
        );
    }
}

#[test]
fn impossible_reserved_namespace_is_an_explicit_error() {
    let mut input = project(
        "._owner{color:red}",
        "<div class=\"_owner\">owner</div>",
        &["_owner"],
    );
    input.reserved_prefixes = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
        .chars()
        .map(|letter| letter.to_string())
        .collect();
    let error = compile_project(
        input,
        Options {
            mode: Mode::Naming,
            ..Default::default()
        },
    )
    .expect_err("all candidate short-name prefixes are reserved");
    assert!(
        matches!(error, lightningcss_compact::Error::Inventory(message) if message.contains("namespace") || message.contains("identifier"))
    );
}

#[test]
fn incomplete_inventory_never_changes_class_membership() {
    let mut input = project(
        ".left{color:red;height:100px}.right{color:red;height:100px}",
        "<div class=\"left\">left</div><div class=\"right\">right</div>",
        &["left", "right"],
    );
    input.complete_usage = false;
    let html = input.bindings[0].value.clone();
    let result = compile_project(input, Options::default()).expect("safe incomplete inventory");
    assert_eq!(result.bindings["document"], html);
    for class in ["left", "right"] {
        assert_eq!(result.manifest.classes[class], vec![class]);
    }
}

#[test]
fn mutable_tokens_are_single_names_and_selector_hooks_keep_identity() {
    let mut input = project(
        ".left{color:red;height:100px}.right{color:red;height:100px}",
        "<div class=\"left\">left</div><div class=\"right\">right</div>",
        &["left", "right"],
    );
    input.bindings.extend([
        BindingInput {
            id: "dynamic".into(),
            kind: BindingKind::Token,
            value: "left".into(),
        },
        BindingInput {
            id: "hook".into(),
            kind: BindingKind::Selector,
            value: ".right:has(+ .left)".into(),
        },
    ]);
    let result = compile_project(input, Options::default()).expect("compile observed owners");
    assert_eq!(result.manifest.classes["left"].len(), 1);
    assert_eq!(
        result.bindings["dynamic"],
        result.manifest.identities["left"]
    );
    assert!(result.bindings["hook"].contains(&result.manifest.identities["right"]));
    assert!(result.bindings["hook"].contains(&result.manifest.identities["left"]));
}

#[test]
fn explicit_reserved_names_and_prefixes_do_not_collide_with_generated_names() {
    let mut input = project(
        ".left{color:red}.right{color:red}.a{color:green}",
        "<div class=\"left\">left</div><div class=\"right\">right</div><div class=\"a language-rust\">foreign</div>",
        &["left", "right"],
    );
    input.reserved_classes.insert("a".into());
    input.reserved_prefixes.push("language-".into());
    let result = compile_project(input, Options::default()).expect("compile reserved namespace");
    assert!(result.bindings["document"].contains("a language-rust"));
    for classes in ["left", "right"].map(|owner| &result.manifest.classes[owner]) {
        assert!(!classes
            .iter()
            .any(|class| class == "a" || class.starts_with("language-")));
    }
}

#[test]
fn compile_is_deterministic_and_baseline_changes_no_identifiers() {
    let input = project(
        ".left{color:red;width:120px;height:100px}.right{color:red;width:140px;height:100px}",
        "<div class=\"left\">left</div><div class=\"right\">right</div>",
        &["left", "right"],
    );
    let first = compile_project(input.clone(), Options::default()).expect("first compilation");
    let second = compile_project(input.clone(), Options::default()).expect("second compilation");
    assert_eq!(
        serde_json::to_value(first).expect("serialize first"),
        serde_json::to_value(second).expect("serialize second")
    );
    let html = input.bindings[0].value.clone();
    let baseline = compile_project(
        input,
        Options {
            mode: Mode::Baseline,
            ..Default::default()
        },
    )
    .expect("ordinary Lightning CSS baseline");
    assert_eq!(baseline.bindings["document"], html);
}

#[test]
fn lightningcss_visitor_applies_prepared_plan_and_rejects_different_ast() {
    use lightningcss_compact::lightningcss::stylesheet::{
        ParserOptions, PrinterOptions, StyleSheet,
    };
    let source = ".left{color:red;height:100px}.right{color:red;height:100px}";
    let input = project(
        source,
        "<div class=\"left\">left</div><div class=\"right\">right</div>",
        &["left", "right"],
    );
    let prepared = prepare_project(input, Options::default()).expect("whole-project preparation");
    let mut stylesheet = StyleSheet::parse(source, ParserOptions::default()).expect("client AST");
    apply_plan(&prepared, "fixture.css", &mut stylesheet).expect("visitor application");
    let css = stylesheet
        .to_css(PrinterOptions {
            minify: true,
            ..Default::default()
        })
        .expect("client printing")
        .code;
    assert_eq!(css, prepared.compiled().stylesheets["fixture.css"]);
    let mut changed =
        StyleSheet::parse(".left{color:blue}", ParserOptions::default()).expect("changed AST");
    assert!(apply_plan(&prepared, "fixture.css", &mut changed).is_err());
}
