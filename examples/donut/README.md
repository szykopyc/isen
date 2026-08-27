# Isen donut

A spinning, lit, depth-buffered ASCII torus, written entirely in Isen.
It needs no bespoke Rust extension—the programme owns the renderer.

```sh
./isen examples/donut/donut.is
```

## Files

| File | What it is |
| --- | --- |
| `donut.is` | The programme: projection, lighting, animation loop, HUD |
| `lib/tables.is` | Precomputed sine/cosine wheels backed by `Maths` |
| `lib/canvas.is` | Depth-buffered ASCII framebuffer |

## Why there is no Rust in here

The two things that would tempt you into a native extension both turned out
to be solvable in the language:

**The inner loop.** 72 × 180 points per frame, twice-rotated and lit, is a lot
of interpreted arithmetic. Rather than moving it to Rust, `lib/tables.is`
precomputes every angle once into `arr[float]` wheels. Rotation angles are then
passed around as *integer indices*, so `render_torus` performs zero
trigonometry in its hot loop—only array reads. Turn the step counts down if your terminal
still struggles.

**A mutable framebuffer.** A `Canvas` groups dimensions with two mutable
arrays: `arr[int]` shades and `arr[float]` depths. Nested assignment is legal;
the one-line `poke_shade` / `poke_depth` helpers simply keep writes named and
centralised inside the renderer.

## Other things worth knowing

- Mutating helpers return `unit` and fall through after completing their work.
- The torus is scaled to fit exactly: its widest possible projection is
  `span / sqrt(viewer² - span²)`, so the fit is computed rather than guessed,
  with `scale_x = 2 × scale_y` for character aspect ratio.
- Back faces are discarded before they reach the depth buffer, so half the
  points cost nothing.
- The frame loop measures itself with `Time.clock` / `Time.since` and sleeps
  only the remainder of its 40 ms budget, so slower terminals degrade in frame
  rate rather than desynchronising.
- `LengText.flush()` clears the screen each frame; `LengText` recolours the whole torus
  every 60 frames because it seemed rude not to.

## Knobs

In `donut.is`: `view_width`, `view_height`, `ring_steps`, `sweep_steps`,
`total_frames`, `frame_budget`, and the `palette`, which runs from blank to
glaring. Swapping the palette for `[" ", ".", "o", "O", "0"]` gets you a
pleasantly bubbly donut.
