# MetroQueue Desktop — Material 3 Expressive plan (agreed)
Decisions:
- M3 Expressive, like the phone app: bolder shapes, springy motion, wavy progress, bigger type.
- Keep the current icons (hugeicons).
- Replace Limusic's theme options with M3 only: seed colour + dynamic/artwork colour + pure black.

1. Foundations (first, one agent):
   - M3 scheme from a seed via @material/material-color-utilities (tonal palettes, all roles incl. surface-container levels), light + dark + pure black;
   - seed = chosen accent or artwork colour (reuse artcolor.ts);
   - map the existing shadcn CSS vars in ui/src/routes/layout.css onto M3 roles;
   - M3 type scale in Google Sans (fontsource already bundled);
   - shape tokens (4/8/12/16/28/full);
   - state layers, motion (expressive springs/easings);
   - Appearance settings reduced to: seed colour, dynamic (artwork) colour, theme (system/light/dark), pure black.
2. Components in parallel, ui/src/lib/components/ui/* + specific components:
   - A controls: button variants (filled/tonal/outlined/text/icon, pill), switch with thumb icons, slider (expressive), chips, inputs, checkbox/radio.
   - B navigation: sidebar → M3 nav rail/drawer with a pill indicator; top app bar; search bar; player bar like the phone.
   - C overlays/lists: dialog (28 px), menus, snackbar, cards, list rows, tabs, progress (wavy).
3. Polish pass on custom screens: Home, Now Playing, Settings, Vibes, lyrics, etc.
Check each step with Playwright screenshots (mock backend) before pushing to dev. Keep changes concentrated in tokens + primitives for upstream merges.
