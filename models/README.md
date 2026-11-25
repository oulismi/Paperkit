# Models

Downloaded, never committed. `node scripts/fetch-model.mjs` puts them here for development
and tests; the app downloads its own copy into the user's app-data folder on first use.

## gliner_multi-v2.1/ — names, addresses, free-text categories

- Model: GLiNER multilingual v2.1 by urchade (Apache-2.0), ONNX export
  `onnx-community/gliner_multi-v2.1`, pinned to revision `6ddaeb94`.
- Files: `tokenizer.json` (16 MB) and `onnx/model_fp16.onnx` (580 MB), checked by SHA-256.
- Why fp16: the smaller int8 `model_quantized.onnx` (349 MB) is broken for this model — it
  scores every word ~10% and finds nothing. fp16 gives the same results as full precision.
  A custom int8 quantization could bring it near 350 MB later.

## Planned

- `ocr/` — OCR for scanned pages (English, French, Dutch, Greek)
