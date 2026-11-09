
    // P5 train 11, the catalog join test on merge 1 (no CATALOG-REFRESH):
    // its two helpers as that side's types have them -- a catalog's due an
    // `Instant` on the process's clock, not a `CatalogDue`. Test code only.
    fn catalog_due(after: Duration) -> Instant {
        Instant::now() + after
    }

    fn catalog_due_ago(ago: Duration) -> Instant {
        Instant::now().checked_sub(ago).expect("a clock older than that")
    }
