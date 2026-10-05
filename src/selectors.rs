use crate::Error;
use lightningcss::{
    properties::custom::{Function, Token, TokenList, TokenOrValue},
    selector::{Component, PseudoClass, PseudoElement, Selector, SelectorList},
    stylesheet::{MinifyOptions, ParserOptions, PrinterOptions, StyleSheet},
    traits::{ParseWithOptions, ToCss},
    visitor::{Visit, VisitTypes, Visitor},
};
use parcel_selectors::attr::{
    AttrSelectorOperator, CaseSensitivity, ParsedAttrSelectorOperation, ParsedCaseSensitivity,
};
use std::collections::{BTreeMap, BTreeSet};

// alpha.72 keeps deduplication keys that refer to mutable rule-vector indices.
// Adjacent merging mutates cached keys and may pop an index which a later rule
// reuses. Randomized hash-table probing can then delete that later rule.
// Temporary barriers prevent key mutation and index reuse; deterministic
// factoring supplies shared rules before the minifier runs. The barriers never
// reach emitted CSS or source maps.
pub(crate) fn minify_stable(sheet: &mut StyleSheet<'_>, id: &str) -> Result<(), Error> {
    const BARRIER: &str = "--lightningcss-compact-internal-boundary";
    struct Boundaries(bool);
    impl<'i> Visitor<'i> for Boundaries {
        type Error = Error;
        fn visit_types(&self) -> VisitTypes {
            VisitTypes::RULES
        }
        fn visit_rule_list(
            &mut self,
            list: &mut lightningcss::rules::CssRuleList<'i>,
        ) -> Result<(), Error> {
            use lightningcss::rules::{unknown::UnknownAtRule, CssRule, Location};
            if self.0 {
                let mut rules = Vec::with_capacity(list.0.len() + list.0.len() / 2);
                let mut run = 0;
                for rule in std::mem::take(&mut list.0) {
                    if matches!(rule, CssRule::Style(_)) {
                        if run == 1 {
                            rules.push(CssRule::Unknown(UnknownAtRule {
                                name: BARRIER.into(),
                                prelude: lightningcss::properties::custom::TokenList(Vec::new()),
                                block: None,
                                loc: Location {
                                    source_index: u32::MAX,
                                    line: 0,
                                    column: 1,
                                },
                            }));
                            run = 0;
                        }
                        run += 1;
                    } else {
                        run = 0;
                    }
                    rules.push(rule);
                }
                list.0 = rules;
            } else {
                list.0.retain(|rule| {
                    !matches!(rule, CssRule::Unknown(unknown)
                        if unknown.name.as_ref() == BARRIER && unknown.loc.source_index == u32::MAX)
                });
            }
            list.visit_children(self)
        }
    }
    sheet.visit(&mut Boundaries(true))?;
    sheet
        .minify(MinifyOptions::default())
        .map_err(|e| Error::Css {
            stylesheet: id.into(),
            message: e.to_string(),
        })?;
    sheet.visit(&mut Boundaries(false))
}

pub(crate) fn parse<'a>(css: &'a str, id: &str) -> Result<StyleSheet<'a>, Error> {
    StyleSheet::parse(
        css,
        ParserOptions {
            filename: id.into(),
            ..Default::default()
        },
    )
    .map_err(|e| Error::Css {
        stylesheet: id.into(),
        message: e.to_string(),
    })
}
pub(crate) fn print_css(sheet: &StyleSheet<'_>, id: &str, minify: bool) -> Result<String, Error> {
    let mut sheet = sheet.clone();
    if minify {
        minify_stable(&mut sheet, id)?;
    }
    sheet
        .to_css(PrinterOptions {
            minify: true,
            ..Default::default()
        })
        .map(|r| r.code)
        .map_err(|e| Error::Css {
            stylesheet: id.into(),
            message: e.to_string(),
        })
}
pub(crate) fn canonical_css(css: &str, id: &str) -> Result<String, Error> {
    print_css(&parse(css, id)?, id, false)
}

pub(crate) fn source_map(
    sheet: &StyleSheet<'_>,
    id: &str,
    source: &str,
) -> Result<(String, String), Error> {
    let mut sheet = sheet.clone();
    minify_stable(&mut sheet, id)?;
    let mut map = parcel_sourcemap::SourceMap::new("/");
    let index = map.add_source(id);
    map.set_source_content(index as usize, source)
        .map_err(|e| Error::Css {
            stylesheet: id.into(),
            message: e.to_string(),
        })?;
    let output = sheet
        .to_css(PrinterOptions {
            minify: true,
            source_map: Some(&mut map),
            ..Default::default()
        })
        .map_err(|e| Error::Css {
            stylesheet: id.into(),
            message: e.to_string(),
        })?;
    let json = map.to_json(None).map_err(|e| Error::Css {
        stylesheet: id.into(),
        message: e.to_string(),
    })?;
    Ok((output.code, json))
}

pub(crate) fn walk(selector: &mut Selector<'_>, callback: &mut impl FnMut(&mut Component<'_>)) {
    for component in selector.iter_mut_raw_match_order() {
        callback(component);
        match component {
            Component::Is(list)
            | Component::Where(list)
            | Component::Negation(list)
            | Component::Has(list)
            | Component::Any(_, list) => {
                for item in list.iter_mut() {
                    walk(item, callback);
                }
            }
            Component::Slotted(item) | Component::Host(Some(item)) => walk(item, callback),
            // NthOf exposes only immutable subselectors in this upstream version.
            // Its names are reserved during inventory, rather than silently missed.
            _ => {}
        }
    }
}

pub(crate) fn classes_in(selector: &Selector<'_>) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut selector = selector.clone();
    walk(&mut selector, &mut |component| {
        if let Component::Class(name) = component {
            found.insert(name.0.to_string());
        }
        if let Component::NthOf(data) = component {
            for item in data.selectors() {
                found.extend(classes_in(item));
            }
        }
        if let Component::PseudoElement(
            PseudoElement::CueFunction { selector } | PseudoElement::CueRegionFunction { selector },
        )
        | Component::NonTSPseudoClass(
            PseudoClass::Local { selector } | PseudoClass::Global { selector },
        ) = component
        {
            found.extend(classes_in(selector));
        }
    });
    found
}

pub(crate) fn simple_owner(selectors: &SelectorList<'_>) -> Option<String> {
    if selectors.0.len() != 1 {
        return None;
    }
    let mut parts = selectors.0[0].iter_raw_match_order();
    match (parts.next(), parts.next()) {
        (Some(Component::Class(name)), None) => Some(name.0.to_string()),
        _ => None,
    }
}

pub(crate) struct Rename<'a>(pub &'a BTreeMap<String, String>);
impl<'i> Visitor<'i> for Rename<'_> {
    type Error = Error;
    fn visit_types(&self) -> VisitTypes {
        VisitTypes::SELECTORS
    }
    fn visit_selector(&mut self, selector: &mut Selector<'i>) -> Result<(), Error> {
        walk(selector, &mut |component| match component {
            Component::Class(name) => {
                if let Some(new) = self.0.get(name.0.as_ref()) {
                    name.0 = new.clone().into();
                }
            }
            Component::AttributeInNoNamespace {
                local_name,
                operator: AttrSelectorOperator::Includes,
                value,
                ..
            } if local_name.0.as_ref() == "class" => {
                if let Some(new) = self.0.get(value.0.as_ref()) {
                    value.0 = new.clone().into();
                }
            }
            _ => {}
        });
        Ok(())
    }
}

pub(crate) fn rewrite_selector(
    value: &str,
    names: &BTreeMap<String, String>,
) -> Result<String, Error> {
    let mut list = SelectorList::parse_string_with_options(value, ParserOptions::default())
        .map_err(|e| Error::Inventory(format!("invalid selector binding {value:?}: {e}")))?;
    for selector in list.0.iter_mut() {
        Rename(names).visit_selector(selector)?;
    }
    list.to_css_string(PrinterOptions {
        minify: true,
        ..Default::default()
    })
    .map_err(|e| Error::Inventory(e.to_string()))
}

pub(crate) fn selector_classes(value: &str) -> Result<BTreeSet<String>, Error> {
    let list = SelectorList::parse_string_with_options(value, ParserOptions::default())
        .map_err(|e| Error::Inventory(format!("invalid selector binding {value:?}: {e}")))?;
    Ok(list.0.iter().flat_map(classes_in).collect())
}

#[derive(Default)]
pub(crate) struct Usage {
    pub all: BTreeSet<String>,
    pub observed: BTreeSet<String>,
    pub reserved: BTreeSet<String>,
    pub attributes: Vec<ClassObservation>,
    pub reads_class_attribute: bool,
    style_context: bool,
}

#[derive(Clone)]
pub(crate) struct ClassObservation {
    operator: AttrSelectorOperator,
    value: String,
    insensitive: bool,
}
impl ClassObservation {
    pub(crate) fn matches(&self, value: &str) -> bool {
        // Empty substring/prefix/suffix selectors never match by CSS rules;
        // they must not forbid every possible generated identifier.
        if self.value.is_empty() && self.operator != AttrSelectorOperator::Equal {
            return false;
        }
        self.operator.eval_str(
            value,
            &self.value,
            if self.insensitive {
                CaseSensitivity::AsciiCaseInsensitive
            } else {
                CaseSensitivity::CaseSensitive
            },
        )
    }
    pub(crate) fn has_whitespace(&self) -> bool {
        self.value.chars().any(char::is_whitespace)
    }
}

pub(crate) struct HtmlClassLists {
    pub enabled: Vec<String>,
    // lol_html parses noscript content as raw text, as a scripting-enabled
    // browser does. Parse each opaque fallback separately for inventory only;
    // its original source must never be rewritten. These names also exist as
    // real elements when scripting is disabled and must remain reserved.
    pub noscript: Vec<String>,
}

pub(crate) fn html_classes(value: &str) -> Result<HtmlClassLists, Error> {
    fn inventory(value: &str, depth: usize) -> Result<HtmlClassLists, Error> {
        if depth > 64 {
            return Err(Error::Inventory(
                "noscript fallback nesting exceeds the 64-level inventory bound".into(),
            ));
        }
        let mut enabled = Vec::new();
        let raw = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let starts = std::rc::Rc::clone(&raw);
        let chunks = std::rc::Rc::clone(&raw);
        lol_html::rewrite_str(
            value,
            lol_html::RewriteStrSettings {
                element_content_handlers: vec![
                    lol_html::element!("[class]", |el| {
                        if let Some(value) = el.get_attribute("class") {
                            enabled.push(value);
                        }
                        Ok(())
                    }),
                    lol_html::element!("noscript", move |_el| {
                        starts.borrow_mut().push(String::new());
                        Ok(())
                    }),
                    lol_html::text!("noscript", move |text| {
                        if text.text_type() == lol_html::html_content::TextType::RawText {
                            if let Some(current) = chunks.borrow_mut().last_mut() {
                                current.push_str(text.as_str());
                            }
                        }
                        Ok(())
                    }),
                ],
                ..Default::default()
            },
        )
        .map_err(|e| Error::Inventory(e.to_string()))?;
        let mut noscript = Vec::new();
        for fallback in raw.take() {
            let inner = inventory(&fallback, depth + 1)?;
            noscript.extend(inner.enabled);
            noscript.extend(inner.noscript);
        }
        Ok(HtmlClassLists { enabled, noscript })
    }
    inventory(value, 0)
}

impl Usage {
    // Some upstream selector subtrees cannot be rewritten by our adapter.
    // Inventory their complete observations and pin class identities instead.
    fn observe_selector(&mut self, selector: &Selector<'_>, immutable: bool) {
        for component in selector.iter_raw_match_order() {
            match component {
                Component::Class(name) => {
                    self.all.insert(name.0.to_string());
                    if !self.style_context || immutable {
                        self.observed.insert(name.0.to_string());
                    }
                    if immutable {
                        self.reserved.insert(name.0.to_string());
                    }
                }
                Component::AttributeInNoNamespace {
                    local_name,
                    operator,
                    value,
                    case_sensitivity,
                    ..
                } if local_name.0.eq_ignore_ascii_case("class") => {
                    let insensitive = matches!(
                        case_sensitivity,
                        ParsedCaseSensitivity::AsciiCaseInsensitive
                            | ParsedCaseSensitivity::AsciiCaseInsensitiveIfInHtmlElementInHtmlDocument
                    );
                    if *operator == AttrSelectorOperator::Includes && !insensitive && !immutable {
                        self.observed.insert(value.0.to_string());
                        self.all.insert(value.0.to_string());
                    } else {
                        self.reserved
                            .extend(value.0.split_ascii_whitespace().map(str::to_string));
                        self.attributes.push(ClassObservation {
                            operator: *operator,
                            value: value.0.to_string(),
                            insensitive,
                        });
                    }
                }
                Component::AttributeOther(attribute)
                    if attribute.local_name.0.eq_ignore_ascii_case("class") =>
                {
                    // Uppercase and namespaced attributes use this upstream
                    // variant. Preserve their values and all matching names.
                    if let ParsedAttrSelectorOperation::WithValue {
                        operator,
                        expected_value,
                        case_sensitivity,
                    } = &attribute.operation
                    {
                        self.reserved.extend(
                            expected_value
                                .0
                                .split_ascii_whitespace()
                                .map(str::to_string),
                        );
                        self.attributes.push(ClassObservation {
                            operator: *operator,
                            value: expected_value.0.to_string(),
                            insensitive: matches!(case_sensitivity,
                                ParsedCaseSensitivity::AsciiCaseInsensitive
                                    | ParsedCaseSensitivity::AsciiCaseInsensitiveIfInHtmlElementInHtmlDocument),
                        });
                    }
                }
                Component::Is(items)
                | Component::Where(items)
                | Component::Negation(items)
                | Component::Has(items)
                | Component::Any(_, items) => {
                    for item in items.iter() {
                        self.observe_selector(item, immutable);
                    }
                }
                Component::Slotted(item) | Component::Host(Some(item)) => {
                    self.observe_selector(item, immutable);
                }
                Component::NthOf(items) => {
                    for item in items.selectors() {
                        self.observe_selector(item, true);
                    }
                }
                Component::PseudoElement(
                    PseudoElement::CueFunction { selector }
                    | PseudoElement::CueRegionFunction { selector },
                )
                | Component::NonTSPseudoClass(
                    PseudoClass::Local { selector } | PseudoClass::Global { selector },
                ) => self.observe_selector(selector, true),
                _ => {}
            }
        }
    }
}

fn attr_may_read_class(arguments: &[TokenOrValue<'_>]) -> bool {
    let mut significant = arguments.iter().filter(|token| {
        !matches!(
            token,
            TokenOrValue::Token(Token::WhiteSpace(_) | Token::Comment(_))
        )
    });
    let Some(TokenOrValue::Token(Token::Ident(name))) = significant.next() else {
        // Null namespaces, substitutions and unknown first-argument syntax
        // cannot prove an unrelated literal attribute name.
        return true;
    };
    if name.eq_ignore_ascii_case("class") {
        return true;
    }
    match significant.next() {
        // These literal boundaries/types leave the unrelated identifier as the
        // entire attribute name. Namespaces and arbitrary substitutions do not.
        None
        | Some(TokenOrValue::Token(
            Token::Comma | Token::CloseParenthesis | Token::Ident(_) | Token::Delim('%'),
        )) => false,
        Some(TokenOrValue::Function(function)) if function.name.0.eq_ignore_ascii_case("type") => {
            false
        }
        Some(TokenOrValue::Token(Token::Function(name))) if name.eq_ignore_ascii_case("type") => {
            false
        }
        _ => true,
    }
}

impl<'i> Visitor<'i> for Usage {
    type Error = Error;
    fn visit_types(&self) -> VisitTypes {
        VisitTypes::RULES
            | VisitTypes::SELECTORS
            | VisitTypes::SUPPORTS_CONDITIONS
            | VisitTypes::FUNCTIONS
            | VisitTypes::TOKENS
    }
    fn visit_rule(&mut self, rule: &mut lightningcss::rules::CssRule<'i>) -> Result<(), Error> {
        let previous = self.style_context;
        self.style_context = matches!(rule, lightningcss::rules::CssRule::Style(_));
        if let lightningcss::rules::CssRule::Style(style) = rule {
            let names: BTreeSet<_> = style.selectors.0.iter().flat_map(classes_in).collect();
            self.all.extend(names.clone());
            if simple_owner(&style.selectors).is_none() || !style.rules.0.is_empty() {
                self.observed.extend(names);
            }
        }
        let result = rule.visit_children(self);
        self.style_context = previous;
        result
    }
    fn visit_selector(&mut self, selector: &mut Selector<'i>) -> Result<(), Error> {
        self.observe_selector(selector, false);
        Ok(())
    }
    fn visit_function(&mut self, function: &mut Function<'i>) -> Result<(), Error> {
        if function.name.0.eq_ignore_ascii_case("attr")
            && attr_may_read_class(&function.arguments.0)
        {
            self.reads_class_attribute = true;
        }
        function.visit_children(self)
    }
    fn visit_token_list(&mut self, tokens: &mut TokenList<'i>) -> Result<(), Error> {
        // Raw token lists preserve function boundaries rather than Function
        // nodes. CSS escapes are already decoded by the upstream parser.
        for (index, token) in tokens.0.iter().enumerate() {
            if matches!(token, TokenOrValue::Token(Token::Function(name)) if name.eq_ignore_ascii_case("attr"))
                && attr_may_read_class(&tokens.0[index + 1..])
            {
                self.reads_class_attribute = true;
            }
        }
        tokens.visit_children(self)
    }
    fn visit_supports_condition(
        &mut self,
        condition: &mut lightningcss::rules::supports::SupportsCondition<'i>,
    ) -> Result<(), Error> {
        // alpha.72 stores selector() as raw text rather than a Selector AST.
        // Inventory valid selectors, and preserve those names with that text.
        if let lightningcss::rules::supports::SupportsCondition::Selector(value) = condition {
            if let Ok(mut list) =
                SelectorList::parse_string_with_options(value, ParserOptions::default())
            {
                let previous = self.style_context;
                self.style_context = false;
                for selector in list.0.iter_mut() {
                    self.reserved.extend(classes_in(selector));
                    self.visit_selector(selector)?;
                }
                self.style_context = previous;
            }
        }
        condition.visit_children(self)
    }
}

/// Discover actual class selector identifiers through the Lightning CSS AST.
pub fn discover_classes(css: &str) -> Result<BTreeSet<String>, Error> {
    let mut sheet = parse(css, "<inventory>")?;
    let mut usage = Usage::default();
    sheet.visit(&mut usage)?;
    Ok(usage.all)
}
