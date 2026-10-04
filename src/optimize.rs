use crate::{model::*, selectors, size};
use lightningcss::{
    rules::{CssRule, CssRuleList},
    stylesheet::{PrinterOptions, StyleSheet},
    visitor::{Visit, VisitTypes, Visitor},
};
use sha2::{Digest, Sha256};
use static_self::IntoOwned;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Default)]
struct State {
    sheets: BTreeMap<String, StyleSheet<'static>>,
    classes: BTreeMap<String, BTreeSet<String>>,
    next_atom: usize,
    attributes: Vec<selectors::ClassObservation>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Candidate {
    sheet: String,
    list: usize,
    positions: Vec<usize>,
    declarations: Vec<String>,
    owners: Vec<String>,
    shared: bool,
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn css_error(id: &str, message: impl ToString) -> Error {
    Error::Css {
        stylesheet: id.into(),
        message: message.to_string(),
    }
}
fn property_text(
    property: &lightningcss::properties::Property<'_>,
    important: bool,
) -> Result<String, Error> {
    property
        .to_css_string(
            important,
            PrinterOptions {
                minify: true,
                ..Default::default()
            },
        )
        .map_err(|e| css_error("<declaration>", e))
}
fn declarations(rule: &lightningcss::rules::style::StyleRule<'_>) -> Result<Vec<String>, Error> {
    rule.declarations
        .declarations
        .iter()
        .map(|p| property_text(p, false))
        .chain(
            rule.declarations
                .important_declarations
                .iter()
                .map(|p| property_text(p, true)),
        )
        .collect()
}

// Longhand/reset footprints are derived from the parser, not split CSS text.
// Unknown and all-reset declarations are barriers. Logical/physical families
// conservatively overlap under every possible writing mode.
fn footprint(property: &lightningcss::properties::Property<'_>) -> BTreeSet<String> {
    let id = property.property_id();
    let name = id.name();
    if name == "all"
        || name.starts_with("--")
        || matches!(
            property,
            lightningcss::properties::Property::Custom(_)
                | lightningcss::properties::Property::Unparsed(_)
        )
    {
        return BTreeSet::from(["*".into()]);
    }
    let mut result: BTreeSet<String> = id
        .longhands()
        .map(|ids| ids.into_iter().map(|i| i.name().to_string()).collect())
        .unwrap_or_else(|| BTreeSet::from([name.into()]));
    for family in [
        "margin",
        "padding",
        "scroll-margin",
        "scroll-padding",
        "border",
        "inset",
        "font",
        "mask",
        "animation",
        "transition",
        "background",
    ] {
        if name.starts_with(family) {
            result.insert(format!("@{family}"));
        }
    }
    if name.contains("width")
        || name.contains("height")
        || name.contains("inline-size")
        || name.contains("block-size")
    {
        result.insert("@size".into());
    }
    if matches!(name, "top" | "left" | "bottom" | "right") {
        result.insert("@inset".into());
    }
    result
}
fn intersects(a: &BTreeSet<String>, b: &BTreeSet<String>) -> bool {
    a.contains("*") || b.contains("*") || !a.is_disjoint(b)
}
fn rule_writes(
    rule: &lightningcss::rules::style::StyleRule<'_>,
    except: &BTreeSet<String>,
) -> Result<BTreeSet<String>, Error> {
    let mut writes = BTreeSet::new();
    for (props, important) in [
        (&rule.declarations.declarations, false),
        (&rule.declarations.important_declarations, true),
    ] {
        for property in props {
            if !except.contains(&property_text(property, important)?) {
                writes.extend(footprint(property));
            }
        }
    }
    Ok(writes)
}

struct Collect<'a> {
    sheet: &'a str,
    sequence: usize,
    out: Vec<Candidate>,
    managed: &'a BTreeSet<String>,
    dynamic: &'a BTreeSet<String>,
    reserved: &'a BTreeSet<String>,
    limit: usize,
    blocked: usize,
}
impl<'i> Visitor<'i> for Collect<'_> {
    type Error = Error;
    fn visit_types(&self) -> VisitTypes {
        VisitTypes::RULES
    }
    fn visit_rule_list(&mut self, list: &mut CssRuleList<'i>) -> Result<(), Error> {
        let sequence = self.sequence;
        self.sequence += 1;
        let mut rows = Vec::new();
        for (position, rule) in list.0.iter().enumerate() {
            if let CssRule::Style(style) = rule {
                if style.rules.0.is_empty() {
                    if let Some(owner) = selectors::simple_owner(&style.selectors) {
                        let declarations = declarations(style)?;
                        if !declarations.is_empty() {
                            rows.push((position, owner, declarations));
                        }
                    }
                }
            }
        }
        // Mandatory singleton, pair, complete-support and full-owner bundles.
        let mut subsets = BTreeSet::<Vec<String>>::new();
        let mut supports = BTreeMap::<String, Vec<usize>>::new();
        for (index, (_, _, decls)) in rows.iter().enumerate() {
            for declaration in decls.iter().collect::<BTreeSet<_>>() {
                supports.entry(declaration.clone()).or_default().push(index);
            }
            subsets.insert(decls.clone());
        }
        let units: Vec<_> = supports.keys().cloned().collect();
        for unit in &units {
            subsets.insert(vec![unit.clone()]);
        }
        for (a, left) in units.iter().enumerate() {
            for right in units.iter().skip(a + 1) {
                if supports[left]
                    .iter()
                    .filter(|i| supports[right].contains(i))
                    .count()
                    >= 2
                {
                    subsets.insert(vec![left.clone(), right.clone()]);
                }
            }
        }
        let mut by_support = BTreeMap::<Vec<usize>, Vec<String>>::new();
        for (unit, owners) in &supports {
            if owners.len() >= 2 {
                by_support
                    .entry(owners.clone())
                    .or_default()
                    .push(unit.clone());
            }
        }
        subsets.extend(by_support.into_values());
        // Bounded larger intersections; full identical supports above are never capped.
        let mut extra = 0;
        for (a, (_, _, left)) in rows.iter().enumerate() {
            for (_, _, right) in rows.iter().skip(a + 1) {
                if extra >= self.limit {
                    break;
                }
                let common: Vec<_> = left.iter().filter(|d| right.contains(d)).cloned().collect();
                if common.len() > 2 && subsets.insert(common) {
                    extra += 1;
                }
            }
        }
        for subset in subsets {
            let wanted: BTreeSet<_> = subset.iter().cloned().collect();
            let matching: Vec<_> = rows
                .iter()
                .filter(|(_, _, decls)| wanted.iter().all(|d| decls.contains(d)))
                .collect();
            if matching.len() < 2 {
                continue;
            }
            // Retain original fallback/reset ordering inside an extracted bundle.
            let ordered: Vec<_> = matching[0]
                .2
                .iter()
                .filter(|d| wanted.contains(*d))
                .cloned()
                .collect();
            if matching.iter().any(|row| {
                row.2
                    .iter()
                    .filter(|d| wanted.contains(*d))
                    .cloned()
                    .collect::<Vec<_>>()
                    != ordered
            }) {
                continue;
            }
            // Partition at conflicting writers. This interval proof is a
            // conservative DAG contraction: every moved conflicting edge stays ordered.
            let mut run: Vec<&(usize, String, Vec<String>)> = Vec::new();
            for row in matching {
                let split = if let Some(previous) = run.last() {
                    let mut target = BTreeSet::new();
                    if let CssRule::Style(style) = &list.0[previous.0] {
                        for (props, important) in [
                            (&style.declarations.declarations, false),
                            (&style.declarations.important_declarations, true),
                        ] {
                            for property in props {
                                if wanted.contains(&property_text(property, important)?) {
                                    target.extend(footprint(property));
                                }
                            }
                        }
                    }
                    let mut conflict = false;
                    for (position, crossed) in
                        list.0.iter().enumerate().take(row.0 + 1).skip(previous.0)
                    {
                        match crossed {
                            CssRule::Style(style) if style.rules.0.is_empty() => {
                                let exceptions = if position == previous.0 || position == row.0 {
                                    &wanted
                                } else {
                                    &BTreeSet::new()
                                };
                                if intersects(&target, &rule_writes(style, exceptions)?) {
                                    conflict = true;
                                    break;
                                }
                            }
                            _ => {
                                conflict = true;
                                break;
                            }
                        }
                    }
                    conflict
                } else {
                    false
                };
                if split {
                    self.blocked += 1;
                    self.add_run(sequence, &run, &ordered);
                    run.clear();
                }
                run.push(row);
            }
            self.add_run(sequence, &run, &ordered);
        }
        list.visit_children(self)
    }
}
impl Collect<'_> {
    fn add_run(
        &mut self,
        sequence: usize,
        run: &[&(usize, String, Vec<String>)],
        declarations: &[String],
    ) {
        if run.len() < 2 {
            return;
        }
        let positions = run.iter().map(|r| r.0).collect();
        let owners: Vec<_> = run.iter().map(|r| r.1.clone()).collect();
        self.out.push(Candidate {
            sheet: self.sheet.into(),
            list: sequence,
            positions,
            declarations: declarations.into(),
            owners: owners.clone(),
            shared: false,
        });
        if owners.iter().all(|o| {
            self.managed.contains(o) && !self.dynamic.contains(o) && !self.reserved.contains(o)
        }) {
            let mut candidate = self.out.last().expect("just pushed").clone();
            candidate.shared = true;
            self.out.push(candidate);
        }
    }
}

struct Extract<'a> {
    target: &'a Candidate,
    sequence: usize,
    atom: &'a str,
}
impl<'i> Visitor<'i> for Extract<'_> {
    type Error = Error;
    fn visit_types(&self) -> VisitTypes {
        VisitTypes::RULES
    }
    fn visit_rule_list(&mut self, list: &mut CssRuleList<'i>) -> Result<(), Error> {
        let sequence = self.sequence;
        self.sequence += 1;
        if sequence != self.target.list {
            return list.visit_children(self);
        }
        let wanted: BTreeSet<_> = self.target.declarations.iter().cloned().collect();
        let CssRule::Style(first) = &list.0[self.target.positions[0]] else {
            return Err(Error::Inventory("invalid extraction position".into()));
        };
        let mut shared = first.clone();
        if self.target.shared {
            let text = format!(".{}{{}}", self.atom);
            let parsed = selectors::parse(&text, "<generated>")?;
            let CssRule::Style(style) = &parsed.rules.0[0] else {
                unreachable!()
            };
            shared.selectors = style.selectors.clone().into_owned();
        } else {
            for position in self.target.positions.iter().skip(1) {
                if let CssRule::Style(style) = &list.0[*position] {
                    shared.selectors.0.push(style.selectors.0[0].clone());
                }
            }
        }
        shared
            .declarations
            .declarations
            .retain(|p| wanted.contains(&property_text(p, false).expect("already serialized")));
        shared
            .declarations
            .important_declarations
            .retain(|p| wanted.contains(&property_text(p, true).expect("already serialized")));
        for position in &self.target.positions {
            if let CssRule::Style(style) = &mut list.0[*position] {
                style.declarations.declarations.retain(|p| {
                    !wanted.contains(&property_text(p, false).expect("already serialized"))
                });
                style.declarations.important_declarations.retain(|p| {
                    !wanted.contains(&property_text(p, true).expect("already serialized"))
                });
            }
        }
        list.0
            .insert(self.target.positions[0], CssRule::Style(shared));
        // Empty rules stay until final minification, preserving list numbering
        // during traversal; they do not introduce an observable declaration.
        Ok(())
    }
}

fn rewrite_classes(
    value: &str,
    classes: &BTreeMap<String, BTreeSet<String>>,
    names: &BTreeMap<String, String>,
) -> String {
    if value.split_ascii_whitespace().all(|original| {
        classes
            .get(original)
            .is_none_or(|symbols| symbols.len() == 1 && symbols.contains(original))
            && names.get(original).is_none_or(|new| new == original)
    }) {
        return value.into();
    }
    let mut seen = BTreeSet::new();
    let mut output = Vec::new();
    for original in value.split_ascii_whitespace() {
        let symbols = classes
            .get(original)
            .cloned()
            .unwrap_or_else(|| BTreeSet::from([original.into()]));
        for symbol in symbols {
            let token = names.get(&symbol).cloned().unwrap_or(symbol);
            if seen.insert(token.clone()) {
                output.push(token);
            }
        }
    }
    output.join(" ")
}
fn html(
    value: &str,
    classes: &BTreeMap<String, BTreeSet<String>>,
    names: &BTreeMap<String, String>,
) -> Result<String, Error> {
    lol_html::rewrite_str(
        value,
        lol_html::RewriteStrSettings {
            element_content_handlers: vec![lol_html::element!("[class]", |el| {
                if let Some(value) = el.get_attribute("class") {
                    let new = rewrite_classes(&value, classes, names);
                    if new != value {
                        el.set_attribute("class", &new)?;
                    }
                }
                Ok(())
            })],
            ..Default::default()
        },
    )
    .map_err(|e| Error::Inventory(format!("invalid HTML binding: {e}")))
}

fn short_name(mut index: usize) -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut out = Vec::new();
    loop {
        out.push(ALPHABET[index % 52]);
        if index < 52 {
            break;
        }
        index = index / 52 - 1;
    }
    out.reverse();
    String::from_utf8(out).expect("ASCII alphabet")
}
fn is_reserved(name: &str, reserved: &BTreeSet<String>, input: &ProjectInput) -> bool {
    reserved.contains(name) || input.reserved_prefixes.iter().any(|p| name.starts_with(p))
}
fn names(
    state: &State,
    input: &ProjectInput,
    reserved: &BTreeSet<String>,
    renaming: bool,
) -> Result<BTreeMap<String, String>, Error> {
    if !renaming {
        return Ok(BTreeMap::new());
    }
    let mut frequency = BTreeMap::<String, usize>::new();
    for sheet in state.sheets.values() {
        struct Count<'a>(&'a mut BTreeMap<String, usize>);
        impl<'i> Visitor<'i> for Count<'_> {
            type Error = Error;
            fn visit_types(&self) -> VisitTypes {
                VisitTypes::RULES | VisitTypes::SELECTORS
            }
            fn visit_rule(&mut self, rule: &mut CssRule<'i>) -> Result<(), Error> {
                if matches!(rule,CssRule::Style(style) if style.declarations.is_empty() && style.rules.0.is_empty())
                {
                    return Ok(());
                }
                rule.visit_children(self)
            }
            fn visit_selector(
                &mut self,
                selector: &mut lightningcss::selector::Selector<'i>,
            ) -> Result<(), Error> {
                selectors::walk(selector, &mut |component| {
                    if let lightningcss::selector::Component::Class(name) = component {
                        *self.0.entry(name.0.to_string()).or_default() += 1;
                    }
                });
                Ok(())
            }
        }
        sheet.clone().visit(&mut Count(&mut frequency))?;
    }
    for binding in &input.bindings {
        match binding.kind {
            BindingKind::Classes => {
                for original in binding.value.split_ascii_whitespace() {
                    for token in state
                        .classes
                        .get(original)
                        .cloned()
                        .unwrap_or_else(|| BTreeSet::from([original.into()]))
                    {
                        *frequency.entry(token).or_default() += 1;
                    }
                }
            }
            BindingKind::Token => {
                *frequency.entry(binding.value.clone()).or_default() += 1;
            }
            BindingKind::Selector => {
                for name in selectors::selector_classes(&binding.value)? {
                    *frequency.entry(name).or_default() += 1;
                }
            }
            BindingKind::Html => {
                lol_html::rewrite_str(
                    &binding.value,
                    lol_html::RewriteStrSettings {
                        element_content_handlers: vec![lol_html::element!("[class]", |el| {
                            if let Some(value) = el.get_attribute("class") {
                                for name in
                                    rewrite_classes(&value, &state.classes, &BTreeMap::new())
                                        .split_ascii_whitespace()
                                {
                                    *frequency.entry(name.into()).or_default() += 1;
                                }
                            }
                            Ok(())
                        })],
                        ..Default::default()
                    },
                )
                .map_err(|e| Error::Inventory(e.to_string()))?;
            }
        }
    }
    let mut ranked: Vec<_> = frequency
        .into_iter()
        .filter(|(name, _)| state.classes.contains_key(name) || name.starts_with("_compact_atom_"))
        .filter(|(name, _)| !is_reserved(name, reserved, input))
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut output = BTreeMap::new();
    let mut next = 0;
    let all_old: BTreeSet<_> = state.classes.keys().cloned().collect();
    for (name, _) in ranked {
        let mut token = short_name(next);
        next += 1;
        while is_reserved(&token, reserved, input)
            || all_old.contains(&token)
            || state
                .attributes
                .iter()
                .any(|attribute| attribute.matches(&token))
        {
            if next >= 1_000_000 {
                return Err(Error::Inventory("generated class namespace exhausted within the deterministic one-million-name allocation bound".into()));
            }
            token = short_name(next);
            next += 1;
        }
        output.insert(name, token);
    }
    Ok(output)
}

type Emitted = (
    BTreeMap<String, String>,
    BTreeMap<String, String>,
    ArtifactSizes,
);
fn emit(
    state: &State,
    input: &ProjectInput,
    names: &BTreeMap<String, String>,
) -> Result<Emitted, Error> {
    let mut sheets = BTreeMap::new();
    for (id, original) in &state.sheets {
        let mut sheet = original.clone();
        sheet.visit(&mut selectors::Rename(names))?;
        sheets.insert(id.clone(), selectors::print_css(&sheet, id, true)?);
    }
    let mut bindings = BTreeMap::new();
    let mut kinds = BTreeMap::new();
    for binding in &input.bindings {
        let value = match binding.kind {
            BindingKind::Classes => rewrite_classes(&binding.value, &state.classes, names),
            BindingKind::Html => html(&binding.value, &state.classes, names)?,
            BindingKind::Selector => selectors::rewrite_selector(&binding.value, names)?,
            BindingKind::Token => names
                .get(&binding.value)
                .cloned()
                .unwrap_or_else(|| binding.value.clone()),
        };
        bindings.insert(binding.id.clone(), value);
        kinds.insert(binding.id.clone(), binding.kind);
    }
    let sizes = size::measure(&sheets, &bindings, &kinds)?;
    Ok((sheets, bindings, sizes))
}
fn better(candidate: &ArtifactSizes, current: &ArtifactSizes) -> bool {
    candidate.total.gzip <= current.total.gzip
        && (
            candidate.total.brotli,
            candidate.total.gzip,
            candidate.total.raw,
        ) < (current.total.brotli, current.total.gzip, current.total.raw)
}

fn candidate_state(
    state: &State,
    candidate: &Candidate,
    observed: &BTreeSet<String>,
) -> Result<State, Error> {
    let mut state = state.clone();
    let mut atom = format!("_compact_atom_{}", state.next_atom);
    state.next_atom += 1;
    while state.classes.contains_key(&atom) {
        atom = format!("_compact_atom_{}", state.next_atom);
        state.next_atom += 1;
    }
    state
        .sheets
        .get_mut(&candidate.sheet)
        .expect("known sheet")
        .visit(&mut Extract {
            target: candidate,
            sequence: 0,
            atom: &atom,
        })?;
    if candidate.shared {
        for owner in &candidate.owners {
            state
                .classes
                .get_mut(owner)
                .expect("managed owner")
                .insert(atom.clone());
        }
        let mut still_used = BTreeSet::new();
        for sheet in state.sheets.values() {
            struct Remaining<'a>(&'a mut BTreeSet<String>);
            impl<'i> Visitor<'i> for Remaining<'_> {
                type Error = Error;
                fn visit_types(&self) -> VisitTypes {
                    VisitTypes::RULES
                }
                fn visit_rule(&mut self, rule: &mut CssRule<'i>) -> Result<(), Error> {
                    if let CssRule::Style(s) = rule {
                        if !s.declarations.is_empty() {
                            self.0
                                .extend(s.selectors.0.iter().flat_map(selectors::classes_in));
                        }
                    }
                    rule.visit_children(self)
                }
            }
            sheet.clone().visit(&mut Remaining(&mut still_used))?;
        }
        for owner in &candidate.owners {
            if !observed.contains(owner) && !still_used.contains(owner) {
                state.classes.get_mut(owner).expect("owner").remove(owner);
            }
        }
    }
    Ok(state)
}

fn permutations<T: Clone>(values: &[T]) -> Vec<Vec<T>> {
    if values.len() < 2 {
        return vec![values.to_vec()];
    }
    let mut result = Vec::new();
    for index in 0..values.len() {
        let rest: Vec<_> = values
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != index)
            .map(|(_, v)| v.clone())
            .collect();
        for mut tail in permutations(&rest) {
            tail.insert(0, values[index].clone());
            result.push(tail);
        }
    }
    result
}

// Exact, deliberately finite domain: one sheet, two unobserved managed owners,
// each with the same one or two independent declarations. Enumerate every
// reachable safe contraction, rule/declaration order and first-K naming
// permutation, including temporarily larger representations. No size bound
// prunes a feasible branch. Broader inputs use the bounded measured search.
fn tiny_domain(
    state: &State,
    input: &ProjectInput,
    observed: &BTreeSet<String>,
    reserved: &BTreeSet<String>,
) -> Result<bool, Error> {
    if !input.complete_usage
        || state.sheets.len() != 1
        || input.bindings.iter().map(|b| b.value.len()).sum::<usize>() > 8192
    {
        return Ok(false);
    }
    let sheet = state.sheets.values().next().expect("one sheet");
    if sheet.rules.0.len() != 2 {
        return Ok(false);
    }
    let mut owners = BTreeSet::new();
    let mut units = None;
    for rule in &sheet.rules.0 {
        let CssRule::Style(style) = rule else {
            return Ok(false);
        };
        let Some(owner) = selectors::simple_owner(&style.selectors) else {
            return Ok(false);
        };
        if !input.managed_classes.contains(&owner)
            || observed.contains(&owner)
            || reserved.contains(&owner)
            || !style.rules.0.is_empty()
            || !style.declarations.important_declarations.is_empty()
        {
            return Ok(false);
        }
        owners.insert(owner);
        let decls = declarations(style)?;
        if decls.is_empty()
            || decls.len() > 2
            || decls.iter().collect::<BTreeSet<_>>().len() != decls.len()
        {
            return Ok(false);
        }
        if units.as_ref().is_some_and(|previous| previous != &decls) {
            return Ok(false);
        }
        units = Some(decls);
        let props = &style.declarations.declarations;
        for (index, left) in props.iter().enumerate() {
            for right in props.iter().skip(index + 1) {
                if intersects(&footprint(left), &footprint(right)) {
                    return Ok(false);
                }
            }
        }
    }
    Ok(owners.len() == 2)
}

type ExactResult = (
    State,
    BTreeMap<String, String>,
    ArtifactSizes,
    Vec<Candidate>,
);
fn exhaustive_tiny(
    initial: &State,
    input: &ProjectInput,
    reserved: &BTreeSet<String>,
    observed: &BTreeSet<String>,
    dynamic: &BTreeSet<String>,
    evaluated: &mut usize,
) -> Result<ExactResult, Error> {
    let initial_names = BTreeMap::new();
    let baseline_cost = emit(initial, input, &initial_names)?.2;
    let mut best = (
        initial.clone(),
        initial_names.clone(),
        baseline_cost.clone(),
        Vec::new(),
    );
    let mut queue = vec![(initial.clone(), Vec::new())];
    let mut seen = BTreeSet::new();
    while let Some((state, path)) = queue.pop() {
        let id = state.sheets.keys().next().expect("tiny sheet").clone();
        let key = serde_json::to_vec(&(
            selectors::print_css(&state.sheets[&id], &id, false)?,
            &state.classes,
            state.next_atom,
        ))?;
        if !seen.insert(key) {
            continue;
        }
        let sheet = &state.sheets[&id];
        let rules: Vec<_> = sheet
            .rules
            .0
            .iter()
            .filter(|rule| !matches!(rule,CssRule::Style(style) if style.declarations.is_empty()))
            .cloned()
            .collect();
        let naming = names(&state, input, reserved, true)?;
        let symbols: Vec<_> = naming.keys().cloned().collect();
        let tokens: Vec<_> = naming.values().cloned().collect();
        for rule_order in permutations(&rules) {
            let mut variants = vec![rule_order];
            for index in 0..rules.len() {
                let mut next = Vec::new();
                for variant in variants {
                    let CssRule::Style(style) = &variant[index] else {
                        unreachable!()
                    };
                    for declarations in permutations(&style.declarations.declarations) {
                        let mut changed = variant.clone();
                        if let CssRule::Style(style) = &mut changed[index] {
                            style.declarations.declarations = declarations;
                        }
                        next.push(changed);
                    }
                }
                variants = next;
            }
            for variant in variants {
                let mut proposed = state.clone();
                proposed.sheets.get_mut(&id).expect("sheet").rules.0 = variant;
                for allocation in permutations(&tokens) {
                    let mapping = symbols.iter().cloned().zip(allocation).collect();
                    let cost = emit(&proposed, input, &mapping)?.2;
                    *evaluated += 1;
                    if cost.total.gzip <= baseline_cost.total.gzip
                        && (cost.total.brotli, cost.total.gzip, cost.total.raw)
                            < (best.2.total.brotli, best.2.total.gzip, best.2.total.raw)
                    {
                        best = (proposed.clone(), mapping, cost, path.clone());
                    }
                }
            }
        }
        let mut collect = Collect {
            sheet: &id,
            sequence: 0,
            out: Vec::new(),
            managed: &input.managed_classes,
            dynamic,
            reserved,
            limit: usize::MAX,
            blocked: 0,
        };
        sheet.clone().visit(&mut collect)?;
        collect.out.sort();
        collect.out.dedup();
        for candidate in collect.out {
            let proposed = candidate_state(&state, &candidate, observed)?;
            let mut next_path = path.clone();
            next_path.push(candidate);
            queue.push((proposed, next_path));
        }
    }
    Ok(best)
}

type PlannedOutput = (CompiledProject, BTreeMap<String, StyleSheet<'static>>);
pub(crate) fn compile(input: &ProjectInput, options: &Options) -> Result<PlannedOutput, Error> {
    if input.reserved_prefixes.iter().any(String::is_empty) {
        return Err(Error::Inventory("reserved prefixes cannot be empty".into()));
    }
    if options.search_limit > 4096
        || options.name_swap_limit > 128
        || options.max_evaluations > 65536
        || options.max_rounds > 256
    {
        return Err(Error::Inventory(
            "search bounds exceed supported limits".into(),
        ));
    }
    let mut state = State::default();
    let mut usage = selectors::Usage::default();
    for sheet in &input.stylesheets {
        if sheet.id.is_empty() || state.sheets.contains_key(&sheet.id) {
            return Err(Error::Inventory(format!(
                "duplicate/empty stylesheet identity {:?}",
                sheet.id
            )));
        }
        let mut parsed = selectors::parse(&sheet.source, &sheet.id)?.into_owned();
        parsed.visit(&mut usage)?;
        state.sheets.insert(sheet.id.clone(), parsed);
    }
    let mut ids = BTreeSet::new();
    let mut dynamic = input.dynamic_classes.clone();
    let mut observed = usage.observed.clone();
    let mut class_lists = Vec::new();
    for binding in &input.bindings {
        if binding.id.is_empty() || !ids.insert(&binding.id) {
            return Err(Error::Inventory(format!(
                "duplicate/empty binding {}",
                binding.id
            )));
        }
        match binding.kind {
            BindingKind::Token => {
                if binding.value.is_empty()
                    || binding.value.bytes().any(|byte| byte.is_ascii_whitespace())
                {
                    return Err(Error::Binding {
                        binding: binding.id.clone(),
                        message: "token binding requires one class".into(),
                    });
                }
                dynamic.insert(binding.value.clone());
                usage.all.insert(binding.value.clone());
            }
            BindingKind::Selector => {
                let names = selectors::selector_classes(&binding.value)?;
                observed.extend(names.clone());
                usage.all.extend(names);
                let mut selector_sheet =
                    selectors::parse(&format!("{}{{}}", binding.value), "<selector binding>")?
                        .into_owned();
                selector_sheet.visit(&mut usage)?;
            }
            BindingKind::Classes => {
                usage
                    .all
                    .extend(binding.value.split_ascii_whitespace().map(str::to_string));
                class_lists.push(binding.value.clone());
            }
            BindingKind::Html => {
                let lists = selectors::html_classes(&binding.value)?;
                for list in lists.noscript {
                    usage
                        .reserved
                        .extend(list.split_ascii_whitespace().map(str::to_string));
                    usage
                        .all
                        .extend(list.split_ascii_whitespace().map(str::to_string));
                    class_lists.push(list);
                }
                for list in lists.enabled {
                    usage
                        .all
                        .extend(list.split_ascii_whitespace().map(str::to_string));
                    class_lists.push(list);
                }
            }
        }
    }
    for group in &input.load_groups {
        for id in group {
            if !state.sheets.contains_key(id) {
                return Err(Error::Inventory(format!(
                    "loading group refers to missing sheet {id}"
                )));
            }
        }
    }
    observed.extend(usage.observed.clone());
    let mut reserved = input.reserved_classes.clone();
    reserved.extend(usage.reserved);
    observed.extend(dynamic.clone());
    for attribute in &usage.attributes {
        // Whole class-string comparisons retain ordering, duplicate tokens and
        // membership. Partial-token observations also constrain fresh names.
        if attribute.has_whitespace() {
            reserved.extend(usage.all.clone());
        }
        reserved.extend(
            usage
                .all
                .iter()
                .filter(|name| attribute.matches(name))
                .cloned(),
        );
        for list in &class_lists {
            let normalized = list.split_ascii_whitespace().collect::<Vec<_>>().join(" ");
            if attribute.matches(list) || attribute.matches(list) != attribute.matches(&normalized)
            {
                reserved.extend(list.split_ascii_whitespace().map(str::to_string));
            }
        }
    }
    state.attributes = usage.attributes;
    for name in &usage.all {
        if !input.managed_classes.contains(name) {
            reserved.insert(name.clone());
        }
    }
    reserved.extend(
        usage
            .all
            .iter()
            .filter(|n| input.reserved_prefixes.iter().any(|p| n.starts_with(p)))
            .cloned(),
    );
    observed.extend(reserved.clone());
    for name in &usage.all {
        state
            .classes
            .insert(name.clone(), BTreeSet::from([name.clone()]));
    }
    let mut baseline_state = state.clone();
    let (baseline_css, baseline_bindings, baseline) =
        emit(&baseline_state, input, &BTreeMap::new())?;
    let renaming = input.complete_usage && options.mode != Mode::Baseline;
    let mut names = names(&state, input, &reserved, renaming)?;
    let (_, _, naming) = emit(&state, input, &names)?;
    let mut current = naming.clone();
    let mut report = TransformationReport {
        baseline: baseline.clone(),
        naming: naming.clone(),
        ..Default::default()
    };
    report.diagnostics.push(Diagnostic {
        stylesheet: None,
        reason: "Lightning CSS alpha.72 minification uses temporary rule boundaries to avoid stale deduplication keys; boundaries are removed before printing in every mode".into(),
    });
    if !input.complete_usage && options.mode != Mode::Baseline {
        report.diagnostics.push(Diagnostic {
            stylesheet: None,
            reason: "No complete usage inventory: class identifiers and membership preserved"
                .into(),
        });
    }
    let exact = options.mode == Mode::Compact && tiny_domain(&state, input, &observed, &reserved)?;
    if exact {
        let (proposed, proposed_names, cost, path) = exhaustive_tiny(
            &state,
            input,
            &reserved,
            &observed,
            &dynamic,
            &mut report.candidates_evaluated,
        )?;
        state = proposed;
        names = proposed_names;
        current = cost;
        report.accepted_transformations = path.len();
        report.shared_classes = path.iter().filter(|c| c.shared).count();
        report.extracted_declarations = path
            .iter()
            .filter(|c| c.shared)
            .map(|c| c.declarations.len() * c.owners.len())
            .sum();
        report.diagnostics.push(Diagnostic{stylesheet:None,reason:"exhaustive finite search: two static owners, one/two independent shared declarations; all safe contractions and rule/declaration/first-K name permutations".into()});
    } else if options.mode == Mode::Compact {
        let mut rounds = 0;
        loop {
            if rounds >= options.max_rounds
                || report.candidates_evaluated >= options.max_evaluations
            {
                report.diagnostics.push(Diagnostic{stylesheet:None,reason:"bounded structural search budget reached; retaining best measured representation".into()});
                break;
            }
            rounds += 1;
            let mut candidates = Vec::new();
            for (id, sheet) in &state.sheets {
                let mut collect = Collect {
                    sheet: id,
                    sequence: 0,
                    out: Vec::new(),
                    managed: &input.managed_classes,
                    dynamic: &dynamic,
                    reserved: &reserved,
                    limit: options.search_limit,
                    blocked: 0,
                };
                sheet.clone().visit(&mut collect)?;
                if collect.blocked > 0 && report.accepted_transformations == 0 {
                    report.diagnostics.push(Diagnostic{stylesheet:Some(id.clone()),reason:format!("rejected {} extraction intervals crossing conflicting property writes or rule boundaries",collect.blocked)});
                }
                candidates.extend(collect.out);
            }
            candidates.sort();
            candidates.dedup();
            let mut best = None;
            for candidate in candidates {
                if report.candidates_evaluated >= options.max_evaluations {
                    break;
                }
                if candidate.shared && !input.complete_usage {
                    continue;
                }
                let proposed = candidate_state(&state, &candidate, &observed)?;
                let proposed_names = names_fn(&proposed, input, &reserved, renaming)?;
                let (_, _, cost) = emit(&proposed, input, &proposed_names)?;
                report.candidates_evaluated += 1;
                if better(&cost, &current)
                    && best
                        .as_ref()
                        .is_none_or(|(_, _, best_cost, _)| better(&cost, best_cost))
                {
                    best = Some((proposed, proposed_names, cost, candidate));
                }
            }
            let Some((proposed, proposed_names, cost, candidate)) = best else {
                report.diagnostics.push(Diagnostic{stylesheet:None,reason:"remaining safe candidates do not improve measured combined Brotli/gzip cost".into()});
                break;
            };
            state = proposed;
            names = proposed_names;
            current = cost;
            report.accepted_transformations += 1;
            if candidate.shared {
                report.shared_classes += 1;
                report.extracted_declarations +=
                    candidate.declarations.len() * candidate.owners.len();
            }
            report.diagnostics.push(Diagnostic {
                stylesheet: Some(candidate.sheet),
                reason: format!(
                    "accepted {} of {} declaration(s) for {} owners",
                    if candidate.shared {
                        "shared class"
                    } else {
                        "selector grouping"
                    },
                    candidate.declarations.len(),
                    candidate.owners.len()
                ),
            });
        }
    }
    if renaming && !exact {
        // Allocation order is the frequency ranking, rather than lexical order
        // of the authored identifiers. Consider the most frequent names first.
        let mut ranked: Vec<_> = names.iter().collect();
        ranked.sort_by_key(|(_, token)| {
            token.bytes().fold(0usize, |rank, byte| {
                rank * 52
                    + if byte.is_ascii_lowercase() {
                        usize::from(byte - b'a') + 1
                    } else {
                        usize::from(byte - b'A') + 27
                    }
            })
        });
        let symbols: Vec<_> = ranked
            .into_iter()
            .take(options.name_swap_limit)
            .map(|(name, _)| name.clone())
            .collect();
        for (index, left) in symbols.iter().enumerate() {
            for right in symbols.iter().skip(index + 1) {
                let mut swapped = names.clone();
                let a = swapped[left].clone();
                let b = swapped[right].clone();
                swapped.insert(left.clone(), b);
                swapped.insert(right.clone(), a);
                let (_, _, cost) = emit(&state, input, &swapped)?;
                report.candidates_evaluated += 1;
                if better(&cost, &current) {
                    names = swapped;
                    current = cost;
                }
            }
        }
    }
    let (mut css, mut bindings, mut optimized) = emit(&state, input, &names)?;
    if options.mode != Mode::Baseline
        && (optimized.total.brotli > baseline.total.brotli
            || optimized.total.gzip > baseline.total.gzip)
    {
        report.diagnostics.push(Diagnostic {
            stylesheet: None,
            reason: "compressed inventory grew; preserving baseline output".into(),
        });
        css = baseline_css;
        bindings = baseline_bindings;
        optimized = baseline;
        state = std::mem::take(&mut baseline_state);
        names.clear();
        report.accepted_transformations = 0;
        report.shared_classes = 0;
        report.extracted_declarations = 0;
    }
    report.elided_identities = state
        .classes
        .iter()
        .filter(|(name, symbols)| !symbols.contains(*name))
        .count();
    report.optimized = optimized;
    let classes = state
        .classes
        .iter()
        .map(|(name, symbols)| {
            (
                name.clone(),
                symbols
                    .iter()
                    .map(|s| names.get(s).cloned().unwrap_or_else(|| s.clone()))
                    .collect(),
            )
        })
        .collect();
    let identities = state
        .classes
        .iter()
        .filter(|(name, symbols)| symbols.contains(*name))
        .map(|(name, _)| {
            (
                name.clone(),
                names.get(name).cloned().unwrap_or_else(|| name.clone()),
            )
        })
        .collect();
    let source_hashes = input
        .stylesheets
        .iter()
        .map(|s| (s.id.clone(), hash(s.source.as_bytes())))
        .collect();
    let output_hashes = css
        .iter()
        .map(|(id, css)| (id.clone(), hash(css.as_bytes())))
        .collect();
    let generation = hash(&serde_json::to_vec(&(
        &css,
        &bindings,
        &classes,
        &input.load_groups,
    ))?);
    let chunks = input
        .load_groups
        .iter()
        .enumerate()
        .map(|(index, sheets)| (format!("load-group-{index}"), sheets.clone()))
        .collect();
    let manifest = Manifest {
        schema_version: 1,
        compiler_version: env!("CARGO_PKG_VERSION").into(),
        generation,
        identities,
        classes,
        source_hashes,
        output_hashes,
        load_groups: input.load_groups.clone(),
        chunks,
    };
    let mut source_maps = BTreeMap::new();
    let mut planned_sheets = BTreeMap::new();
    for source in &input.stylesheets {
        let mut sheet = state.sheets[&source.id].clone();
        sheet.visit(&mut selectors::Rename(&names))?;
        let (mapped_css, map) = selectors::source_map(&sheet, &source.id, &source.source)?;
        if mapped_css != css[&source.id] {
            return Err(Error::Inventory(
                "source mapping changed generated CSS".into(),
            ));
        }
        source_maps.insert(source.id.clone(), map);
        selectors::minify_stable(&mut sheet, &source.id)?;
        planned_sheets.insert(source.id.clone(), sheet);
    }
    Ok((
        CompiledProject {
            stylesheets: css,
            bindings,
            manifest,
            report,
            source_maps,
        },
        planned_sheets,
    ))
}
fn names_fn(
    state: &State,
    input: &ProjectInput,
    reserved: &BTreeSet<String>,
    renaming: bool,
) -> Result<BTreeMap<String, String>, Error> {
    names(state, input, reserved, renaming)
}
