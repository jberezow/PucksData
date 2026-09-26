# Repository communication conventions

Keep this repository independent of downstream product branding. Never name
downstream products in pull request titles or descriptions, comments (including
code comments), or documentation. Use generic terms such as “consumer”,
“downstream application”, or “reader” instead. Use neutral names for consumer
contract fixtures and tests as well.

Do not rewrite applied migrations or rename existing database roles merely to
remove a legacy identifier. Preserve migration checksums and runtime grants.
