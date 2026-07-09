---
name: verify
description: Vérifier UberCron en conditions réelles — lancer l'app Tauri en dev et piloter la fenêtre via l'accessibilité macOS (pas de permission Screen Recording nécessaire).
---

# Vérifier UberCron

## Lancer

```sh
npm run tauri dev   # en arrière-plan ; fenêtre "ubercron" up en ~10 s (pgrep -x ubercron)
```

Si `npm`/`node` échoue avec `_load_nvm` introuvable (vieux snapshot de shell),
contourner avec : `export PATH="$HOME/.nvm/versions/node/v24.13.0/bin:$PATH"`.
Le lazy-loader nvm du `.zshrc` a été corrigé le 2026-07-09 (wrappers autonomes).

## Observer et piloter (sans Screen Recording)

`screencapture` échoue (permission absente), mais **System Events / accessibilité
fonctionne** : la WKWebView expose tout le DOM en AXStaticText/AXButton/AXTextField.

Lire les textes affichés :

```applescript
tell application "System Events" to tell process "ubercron"
  set out to {}
  repeat with e in (entire contents of window 1)
    try
      if role of e is "AXStaticText" then set end of out to (value of e as text)
    end try
  end repeat
  return out
end tell
```

Piloter : `click` sur les AXButton (repérés par `name`, qui reflète le libellé i18n
français par défaut), `set focused ... to true` sur un AXTextField puis `keystroke`
(nécessite `set frontmost to true`). Les inputs React ne réagissent PAS à
`set value` — toujours passer par keystroke.

Index des champs de l'éditeur cron : 1 = nom, 2-6 = minute/heure/jour/mois/jour-sem.,
7 = commande.

## Flows qui valent la vérification

- Liste : comparer avec `crontab -l` (le job actif visible, les commentaires absents).
- Aperçu live : ouvrir « Nouveau job », lire l'aperçu (phrase cronstrue + 5 dates) ;
  taper `99` dans minute → « Expression invalide ».
- CRUD réel : sauvegarder `crontab -l` avant, créer un job de test
  (`/bin/echo ubercron-test`), vérifier `crontab -l`, supprimer via l'UI (bouton
  Supprimer → Oui), `diff` avec l'original — doit être identique à l'octet près.
  Backups attendus dans `~/Library/Application Support/UberCron/backups/`.

## Gotchas

- `cargo test` régénère `src/lib/bindings.ts` (voulu).
- Toujours lancer cargo depuis `src-tauri/`.
