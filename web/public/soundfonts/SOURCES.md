# The soundfonts

Written by `scripts/render-soundfonts.mjs`; do not edit by hand. Each `<file>.json` describes one
soundfont's zones — the keys each plays, the pitch its recording sounds at, its loop in the
recording's frames, its rate — and names the zone's recording in `zones/`, a 16-bit FLAC decoded
from the webaudiofont file's MP3 (lossless from there on). A recording two soundfonts share is
stored once, named by the hash of its MP3.

The webaudiofont files come from [felixroos/webaudiofontdata](https://github.com/felixroos/webaudiofontdata) (MIT, [its licence](LICENSES/webaudiofontdata.txt)) at commit `23ca907d4370a04fd89ca483a92915e4d6159ab9`.
The recordings keep their banks' licences, whose full texts and copyright lines are in `LICENSES/`:

- FluidR3 GM by Frank Wen, MIT licence: [LICENSES/FluidR3_GM.txt](LICENSES/FluidR3_GM.txt).
- GeneralUser GS by S. Christian Collins, free for any use (its licence permits use and redistribution, commercial included): [LICENSES/GeneralUser_GS.txt](LICENSES/GeneralUser_GS.txt).

| Instrument | File | Zones | Bank |
|---|---|---|---|
| `gm_acoustic_bass` | [0320_FluidR3_GM_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0320_FluidR3_GM_sf2_file.js) | 10 | FluidR3 GM by Frank Wen, MIT licence |
| `gm_electric_bass_finger` | [0330_GeneralUserGS_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0330_GeneralUserGS_sf2_file.js) | 4 | GeneralUser GS by S. Christian Collins, free for any use (its licence permits use and redistribution, commercial included) |
| `gm_electric_bass_pick` | [0340_FluidR3_GM_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0340_FluidR3_GM_sf2_file.js) | 9 | FluidR3 GM by Frank Wen, MIT licence |
| `gm_synth_bass_1` | [0380_FluidR3_GM_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0380_FluidR3_GM_sf2_file.js) | 4 | FluidR3 GM by Frank Wen, MIT licence |
| `gm_synth_bass_2` | [0390_FluidR3_GM_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0390_FluidR3_GM_sf2_file.js) | 4 | FluidR3 GM by Frank Wen, MIT licence |
| `gm_epiano1` | [0040_FluidR3_GM_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0040_FluidR3_GM_sf2_file.js) | 13 | FluidR3 GM by Frank Wen, MIT licence |
| `gm_epiano2` | [0050_FluidR3_GM_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0050_FluidR3_GM_sf2_file.js) | 9 | FluidR3 GM by Frank Wen, MIT licence |
| `gm_piano` | [0000_FluidR3_GM_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0000_FluidR3_GM_sf2_file.js) | 20 | FluidR3 GM by Frank Wen, MIT licence |
| `gm_vibraphone` | [0110_FluidR3_GM_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0110_FluidR3_GM_sf2_file.js) | 13 | FluidR3 GM by Frank Wen, MIT licence |
| `gm_marimba` | [0120_FluidR3_GM_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0120_FluidR3_GM_sf2_file.js) | 6 | FluidR3 GM by Frank Wen, MIT licence |
| `gm_celesta` | [0080_FluidR3_GM_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0080_FluidR3_GM_sf2_file.js) | 8 | FluidR3 GM by Frank Wen, MIT licence |
| `gm_music_box` | [0100_FluidR3_GM_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0100_FluidR3_GM_sf2_file.js) | 11 | FluidR3 GM by Frank Wen, MIT licence |
| `gm_kalimba` | [1080_FluidR3_GM_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/1080_FluidR3_GM_sf2_file.js) | 6 | FluidR3 GM by Frank Wen, MIT licence |
| `gm_glockenspiel` | [0090_FluidR3_GM_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0090_FluidR3_GM_sf2_file.js) | 8 | FluidR3 GM by Frank Wen, MIT licence |
| `gm_acoustic_guitar_nylon` | [0240_FluidR3_GM_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0240_FluidR3_GM_sf2_file.js) | 10 | FluidR3 GM by Frank Wen, MIT licence |
| `gm_orchestral_harp` | [0460_FluidR3_GM_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0460_FluidR3_GM_sf2_file.js) | 14 | FluidR3 GM by Frank Wen, MIT licence |
| `gm_pad_warm` | [0890_FluidR3_GM_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0890_FluidR3_GM_sf2_file.js) | 2 | FluidR3 GM by Frank Wen, MIT licence |
| `gm_string_ensemble_1` | [0480_FluidR3_GM_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0480_FluidR3_GM_sf2_file.js) | 34 | FluidR3 GM by Frank Wen, MIT licence |
| `gm_cello` | [0420_FluidR3_GM_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0420_FluidR3_GM_sf2_file.js) | 18 | FluidR3 GM by Frank Wen, MIT licence |
| `gm_contrabass` | [0430_GeneralUserGS_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0430_GeneralUserGS_sf2_file.js) | 13 | GeneralUser GS by S. Christian Collins, free for any use (its licence permits use and redistribution, commercial included) |
| `gm_harpsichord` | [0060_FluidR3_GM_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0060_FluidR3_GM_sf2_file.js) | 11 | FluidR3 GM by Frank Wen, MIT licence |
| `gm_church_organ` | [0190_FluidR3_GM_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0190_FluidR3_GM_sf2_file.js) | 21 | FluidR3 GM by Frank Wen, MIT licence |
| `gm_lead_2_sawtooth` | [0810_FluidR3_GM_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0810_FluidR3_GM_sf2_file.js) | 5 | FluidR3 GM by Frank Wen, MIT licence |
| `gm_flute` | [0730_FluidR3_GM_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0730_FluidR3_GM_sf2_file.js) | 9 | FluidR3 GM by Frank Wen, MIT licence |
| `gm_string_ensemble_2` | [0490_FluidR3_GM_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0490_FluidR3_GM_sf2_file.js) | 34 | FluidR3 GM by Frank Wen, MIT licence |
| `gm_synth_strings_1` | [0500_GeneralUserGS_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0500_GeneralUserGS_sf2_file.js) | 5 | GeneralUser GS by S. Christian Collins, free for any use (its licence permits use and redistribution, commercial included) |
| `gm_choir_aahs` | [0520_FluidR3_GM_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0520_FluidR3_GM_sf2_file.js) | 15 | FluidR3 GM by Frank Wen, MIT licence |
| `gm_pad_halo` | [0940_FluidR3_GM_sf2_file](https://github.com/felixroos/webaudiofontdata/blob/23ca907d4370a04fd89ca483a92915e4d6159ab9/sound/0940_FluidR3_GM_sf2_file.js) | 1 | FluidR3 GM by Frank Wen, MIT licence |
