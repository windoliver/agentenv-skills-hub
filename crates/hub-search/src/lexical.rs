use crate::SearchDocument;

pub fn lexical_rank(query: &str, docs: Vec<SearchDocument>) -> Vec<SearchDocument> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }

    let mut ranked = docs
        .into_iter()
        .filter_map(|doc| rank(&query, &doc).map(|score| (score, doc)))
        .collect::<Vec<_>>();

    ranked.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.name.cmp(&right.1.name))
            .then_with(|| left.1.namespace.cmp(&right.1.namespace))
            .then_with(|| left.1.version.cmp(&right.1.version))
    });

    ranked.into_iter().map(|(_, doc)| doc).collect()
}

fn rank(query: &str, doc: &SearchDocument) -> Option<u8> {
    let name = doc.name.to_lowercase();
    let description = doc.description.as_deref().map(str::to_lowercase);

    let score = if name == query {
        4
    } else if name.starts_with(query) {
        3
    } else if name.contains(query) {
        2
    } else if description
        .as_deref()
        .is_some_and(|description| description.contains(query))
    {
        1
    } else {
        0
    };

    (score > 0).then_some(score)
}
