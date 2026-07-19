# Vendored source definitions

Extracted parser config from `nyora-data-driven` — one JSON per kotatsu engine,
each row describing a source (domain, selectors, date pattern, quirks).

Vendored so this repo generates standalone. `nyora-data-driven` remains the
upstream source of truth: it holds the 34 Kotlin engines these were extracted
from, plus the coverage/verification notes. Re-sync with:

    cp ../nyora-data-driven/repo/*.json data/

Six engines are ported so far (madara, mangareader, zeistmanga, hotcomics,
onemanga, mmrcms); the rest are here for when they are.
