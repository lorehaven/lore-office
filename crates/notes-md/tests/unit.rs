use notes_md::{
    Piece, callout, flatten_wikilinks, outline, parse_front, pieces, rename_links,
    set_front_matter_key, slides, slug, tags, title_of, wikilinks,
};

#[quench_test::test]
fn front_matter_is_read_and_set() {
    let text = "---\ntitle: \"Plan, 2026\"\ntheme: night\naspect: 16:9\nowner: me\n---\n# Body\n";
    let (front, body) = parse_front(text);
    assert_eq!(front.title.as_deref(), Some("Plan, 2026"));
    assert_eq!(
        (front.theme.as_deref(), front.aspect.as_deref()),
        (Some("night"), Some("16:9"))
    );
    assert_eq!(front.extra, [("owner".to_string(), "me".to_string())]);
    assert_eq!(body, "# Body\n");
    // Without it, or with an unclosed one, the text is all body.
    assert_eq!(parse_front("# Hi\n").1, "# Hi\\n".replace("\\n", "\n"));
    assert_eq!(
        parse_front("---\ntitle: x\nno end").1,
        "---\ntitle: x\nno end"
    );
    // A rule in the middle is not front matter.
    assert!(parse_front("text\n---\ntitle: x\n---\n").0.title.is_none());
    let set = set_front_matter_key(text, "theme", "day");
    assert!(set.contains("theme: day\n") && !set.contains("night") && set.ends_with("# Body\n"));
    assert_eq!(
        set_front_matter_key("# A\n", "theme", "day"),
        "---\ntheme: day\n---\n# A\n"
    );
    assert!(
        set_front_matter_key("---\ntitle: x\n---\nbody", "theme", "day")
            .contains("title: x\ntheme: day\n---\nbody")
    );
}

#[quench_test::test]
fn slides_split_on_rules_outside_code_with_notes_and_directives() {
    let text = "# One\n\n- a\n- b\n\n???\nSay hello\nslowly\n---\n<!-- layout: two-col -->\n## Two\n\n```\n---\nnot a split\n```\n\nNote: remember\n---\n\n---\nThree\n";
    let s = slides(text);
    assert_eq!(s.len(), 3, "{s:#?}");
    assert_eq!(s[0].markdown, "# One\n\n- a\n- b");
    assert_eq!(s[0].notes, "Say hello\nslowly");
    assert_eq!(s[1].layout.as_deref(), Some("two-col"));
    assert!(s[1].markdown.contains("not a split") && s[1].markdown.starts_with("## Two"));
    assert_eq!(s[1].notes, "remember");
    assert_eq!(s[2].markdown, "Three");
    assert_eq!(s[0].line, 0);
    assert_eq!(s[1].line, 9);
    // Nothing at all is one empty slide.
    assert_eq!(slides("").len(), 1);
    assert_eq!(slides("\n\n").len(), 1);
}

#[quench_test::test]
fn wikilinks_tags_and_headings_skip_code() {
    let text = "# Title\nSee [[Other note]], [[Plan#Goals|the goals]] and ![[pic.png]].\n`[[not]]` and #ideas, #Work/today.\n```\n[[in code]] #nope\n```\nIssue#12 is not a tag, nor is ## here.\n";
    let links = wikilinks(text);
    assert_eq!(links.len(), 3);
    assert_eq!(
        (
            links[0].target.as_str(),
            links[0].heading.as_deref(),
            links[0].embed
        ),
        ("Other note", None, false)
    );
    assert_eq!(
        (
            links[1].target.as_str(),
            links[1].heading.as_deref(),
            links[1].alias.as_deref()
        ),
        ("Plan", Some("Goals"), Some("the goals"))
    );
    assert!(links[2].embed && links[2].target == "pic.png");
    assert_eq!(links[0].line, 1);
    assert_eq!(tags(text), ["ideas", "work/today"]);
    let o = outline("# A\ntext\n## B ##\n```\n# no\n```\n####### seven\n#nospace\n");
    assert_eq!(
        o.iter()
            .map(|h| (h.level, h.text.as_str(), h.line))
            .collect::<Vec<_>>(),
        [(1, "A", 0), (2, "B", 2)]
    );
}

#[quench_test::test]
fn titles_slugs_and_callouts() {
    assert_eq!(
        title_of("n.md", "---\ntitle: From front\n---\n# Heading"),
        "From front"
    );
    assert_eq!(title_of("n.md", "intro\n# Heading\n"), "Heading");
    assert_eq!(title_of("My note.md", "just text"), "My note");
    assert_eq!(slug("Hello, World! 2"), "hello-world-2");
    let c = callout("[!Warning]- Careful now").unwrap();
    assert_eq!(
        (c.kind.as_str(), c.title.as_str()),
        ("warning", "Careful now")
    );
    assert_eq!(callout("[!note]").unwrap().title, "");
    assert!(callout("just a quote").is_none() && callout("[!]").is_none());
}

#[quench_test::test]
fn links_are_cut_out_flattened_and_renamed() {
    let p = pieces("a [[One]] b ![[pic.png]] c [[Two#H|shown]]");
    assert_eq!(p.len(), 6);
    assert_eq!(p[0], Piece::Text("a ".into()));
    assert!(matches!(&p[1], Piece::Link(l) if l.target == "One" && !l.embed));
    assert!(matches!(&p[3], Piece::Link(l) if l.embed && l.target == "pic.png"));
    assert_eq!(
        pieces("no links [[ ]] here").len(),
        2,
        "an empty one stays text"
    );
    assert_eq!(
        flatten_wikilinks("See [[One]], [[Two#H]] and [[Three|3rd]].\n```\n[[code]]\n```"),
        "See One, Two > H and 3rd.\n```\n[[code]]\n```"
    );
    let renamed = rename_links(
        "[[Old]] [[old#Part]] [[Old|x]] [[Other]] ![[Old]]\n`[[Old]]`",
        "Old",
        "New name",
    );
    assert_eq!(
        renamed,
        "[[New name]] [[New name#Part]] [[New name|x]] [[Other]] ![[New name]]\n`[[New name]]`"
    );
    assert_eq!(
        rename_links("```\n[[Old]]\n```", "Old", "N"),
        "```\n[[Old]]\n```"
    );
}

quench_test::main!();
