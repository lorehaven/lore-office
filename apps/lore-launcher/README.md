# lore-launcher

The start page of lore-office: a tile for each app. Run it with the addresses of the apps:

    cd apps/lore-launcher && quench build --release
    PORT=3000 LORE_DOCS_URL=http://localhost:3001/ LORE_SHEETS_URL=http://localhost:3002/ \
      LORE_NOTES_URL=http://localhost:3003/ LORE_PDF_URL=http://localhost:3004/ dist/lore-launcher-server

Without the variables the links are `/lore-docs/`, `/lore-sheets/`, `/lore-notes/` and `/lore-pdf/`, for when the apps are served under one address by a gateway. The page has the theme menu and the language picker.
