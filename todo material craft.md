# todo material craft

Reference survey of the **Noises** and **Patterns** libraries (Texture generators) in Adobe Substance 3D Designer (version 14.1.1, read from the status bar), taken on 2026-10-20 by dropping each node into a scratch graph (2048 px, 8-bit) and reading its Properties panel. Values below are the **defaults at the moment the node was dropped**. Slider values were read from small screenshots, so a few are approximate (marked "approx"). Nothing was copied from Designer's files; this is a list of what the interface shows. Descriptions of what each node does are my reading of the previews and the parameter names, not Adobe documentation.

## How to read this

Every node has the same four panel sections, in this order: **Base Parameters**, **Attributes**, **Instance Parameters**, **Input Values** (details in "Property panel structure" below). The per-node lists are the **Instance Parameters** only. **Input Values** is where a node's image or value inputs would be; the plain noises have none, the 3D noises take a 3D position map.

Recurring parameters:

| Parameter | What it does |
|---|---|
| Scale | Number of noise cells per tile. Higher = smaller features. Whole numbers keep the result tiling. |
| Disorder | Randomly displaces the lattice so features stop looking regular. 0 = clean. |
| Non Square Expansion | True: features stay square on non-square outputs (the tile is expanded instead of stretched). |
| Angle / Angle Random | Rotation of the pattern in turns/degrees, plus random variation per element. |
| Warp Intensity | Distorts the pattern with a second noise (fluid, creased, liquid, microscope). |
| Balance / Contrast / Invert | Grunge-family tone controls: Balance moves the black/white threshold, Contrast hardens it, Invert flips. |
| Advanced (grunge nodes) | A collapsible group with the node-specific controls; it stays open once expanded. |
| Min Level / Max Level / Roughness / Lacunarity / Global Opacity | Fractal controls: first and last octave, how strongly later octaves contribute, how fast frequency grows per octave, overall weight. |
| Distance Mode / Style | Voronoi family. Distance Mode: Euclidean, Manhattan, Chebyshev, Minkowski. Style: F1, F2, F2-F1, F1*F2, F1/F2, Edge, Random Color. |
| Enable Tiling | 3D noises only: forces the 3D noise to tile in the output. |

## Property panel structure (the categories to mirror in MaterialCraft)

The panel is a vertical stack of collapsible sections. Each section header has a chevron; the three non-instance sections start collapsed on a fresh node, Instance Parameters starts open. Open/closed state persists between nodes.

1. **Base Parameters** (same on every node; each row has a small mode icon with the choices *Relative to Parent* / *Absolute*, and the value is greyed out while it is inherited):
   - Output Size: Width and Height as power-of-two steps (shown as "Parent x 1" when relative), with a link toggle that locks them together.
   - Output Format: 8 Bits per Channel, 16 Bits per Channel, HDR Low Precision (16F), HDR High Precision (32F).
   - Pixel Size: Width and Height (default 1).
   - Pixel Ratio: Stretch, Square (default).
   - Tiling Mode: No Tiling, Horizontal Tiling, Vertical Tiling, H and V Tiling (default).
   - Random Seed: integer slider (default 0) with a randomize button.
2. **Attributes** (metadata, read-only for library nodes): Package (file path with a folder button), Graph instance (pkg:///name?dependency=id), Description (text box; some nodes add a Note), Category (e.g. Generator), Author (Adobe), Author URL, Tags (e.g. Noise, Pattern).
3. **Instance Parameters**: the node's own parameters. Two shapes occur:
   - flat list (most noises, Brick 1/2, Gradient nodes, Weave, Triangle Grid, Splatter), or
   - **sub-groups with their own headings** (Shape Splatter: Pattern, Size, Position, Rotation, Height, Masking; Splatter Circular: Pattern, Position, Size, Rotation, Color; Tile Generator: Pattern, Size, Position, Rotation, Color; Tile Sampler: Pattern, Size, Position, Rotation, Color; Tile Random: Pattern, Split, Size, Interstice, Shape, Position, Color, Mask; Tile Random 2: Size, Shape Aspect - Transform, Slant, Bevel, Mask; grunge nodes: one "Advanced" group).
   - Compound controls render as one labelled block with several sliders: Size X/Y/Z, Offset X/Y/Z, Point 1 / Point 2 (X, Y), Rotation (angle dial with Turns and Degrees), Color (picker with sRGB/Float toggle and H/S/V/A sliders), Gap, Bevel, Height, Slope, Offset (in Brick Generator).
   - Control types seen: slider (float or integer), toggle button (False/True), dropdown, two-button choice (Normal Format: OpenGL / DirectX), colour picker, angle dial, enum-driven sub-sliders (Tile Generator "Pattern Specific").
4. **Input Values**: defaults for unconnected inputs (empty on most generators).

Recurring lists (same entries wherever they appear):

| List | Entries |
|---|---|
| Tiler "Pattern" (Tile Generator / Random / Sampler, Splatter Circular, Arc Pavement) | Image Input (Pattern Input in Tile Random), Square, Disc, Paraboloid, Bell, Gaussian, Thorn, Pyramid, Brick, Gradation, Waves, Half Bell, Ridged Bell, Crescent, Capsule, Cone, Gradation w. offset |
| Rotation (tilers) | 0, 90, 180, 270 |
| Symmetry Random Mode | Horizontal + Vertical, Horizontal, Vertical |
| Blending Mode | Add / Max / Add Sub (Tile Generator); Add/Sub / Max (Tile Random, Tile Sampler); Add / Alpha Blend (Splatter Circular Color); Add/Sub / Alpha Blend (Tile Sampler Color) |
| Colour Parametrisation (Tile Random) | None, Area, Size X, Size Y |
| Colour Parametrisation Affect (colour variants) | RGB+Alpha, RGB only, Alpha only |
| Normal Format | OpenGL, DirectX |

## Families (what they are for)

- **Basic noises:** Perlin Noise, Plasma, Gaussian Noise, Clouds 1-3, Fractal Sum Base, Fractal Sum 1-4, White Noise Fast, Blue Noise Fast.
- **Cell patterns:** Cells 1-4 (cell fills, edges, hardness, random colour source), Voronoi, Voronoi Fractal, BnW Spots 1-3, Gaussian Spots 1-2.
- **3D noises (take a position map, so they can be mapped by UVW or world space):** 3D Perlin Noise, 3D Perlin Noise Fractal, 3D Ridged Noise Fractal, 3D Simplex Noise, 3D Voronoi, 3D Voronoi Fractal, 3D Worley Noise.
- **Directional / streaky:** Directional Noise 1-4, Directional Scratches, Anisotropic Noise, Creased, Fur 1-3, Fluid, Liquid, Messy Fibers 1-3, Waveform 1.
- **Crystal / micro structure:** Crystal 1-2, Microscope View, Moisture Noise, RT Caustics (colour) and RT Caustics 2 (grayscale, ray-traced caustics).
- **Dirt:** Dirt 1-5, Dirt Gradient.
- **Grunge maps:** Grunge Map 001-015 (general wear masks), and the named grunges: Concrete, Damas, Galvanic Large/Small, Leaks, Leaky Paint, Rough Dirty, Rust Fine, Scratches Dirty/Fine/Rough, Shavings, Splashes Dusty, Spots, Spots Dirty.

## Per-node reference (Instance Parameters, defaults as dropped)

## Perlin Noise (noise_perlin_noise) — Generator
Scale 32 (int-ish slider), Disorder 0, Non Square Expansion True.
## Plasma
Scale 32, Disorder 0, Non Square Expansion True.
## Moisture Noise
Scale 1, Disorder 0, Pattern Size X/Y 1/1, Pattern Angle (turns/degrees) 0, Pattern Angle Random 0, Global Opacity 1.6, Non Square Expansion True.
## Microscope View
Scale 32, Warp Intensity 0.5, Disorder 0, Non Square Expansion True.
## Voronoi
Invert False; Scale 8; Size X/Y/Z 1; Offset X/Y/Z 0; Disorder X/Y/Z 1; Distortion Intensity 0; Distortion Scale Multiplier 1; Rounded Curve 0; Distance Scale 1; Distance Mode {Euclidean*, Manhattan, Chebyshev, Minkowski}; Style {F1*, F2, F2-F1, F1*F2, F1/F2, Edge, Random Color}; Non Square Expansion True.
## Voronoi Fractal
Invert False; Scale 8; Size XYZ 1; Offset XYZ 0; Disorder XYZ 1; Distortion Intensity 0; Distortion Scale Multiplier 1; Min Level 0; Max Level 4; Roughness 0.5; Lacunarity 2; Global Opacity 1; Rounded Curve 0; Distance Scale 1; Distance Mode {Euclidean*, Manhattan, Chebyshev, Minkowski}; Blend Mode {Add*, Max, Min}; Style {F1*, F2, F2-F1, F1*F2, F1/F2, Edge, Random Color}; Non Square Expansion True.
## White Noise Fast
No instance parameters (only Random Seed in base parameters).
## Waveform 1
Samples 256; SizeMin 0; SizeMax 1; WaveNumber 2; Noise 0.5; Pattern 6.
## RT Caustics (rt_caustics_color)
Output Color Space {Raw, sRGB*}; Photon Grid Size {Auto*, 512, 1024, 2048, 4096}; Surface Height Scale 0.1; Surface Height Position 0.2; Surface IOR 1.333; Photon Size 10; Dispersion 0.005; Jittering 0; Light Position X/Y 0.5/0.5; Background Color (black); Non Square Expansion True.
## RT Caustics 2 (rt_caustics_grayscale)
Same as RT Caustics without Dispersion and Background Color (grayscale output).
## Grunge Splash (grunge_splashes_dusty)
Balance 0.5; Contrast 0; Invert False; Non Square Expansion True; Advanced: Splashes Quantity 1; Splashes Distortion 0.75; Splash/Dirt Ratio 0.5; Dirt Spread 0.5.
## Grunge Spots
Balance 0.5; Contrast 0; Invert False; Non Square Expansion True; Advanced: Details 0; Coverage 0.65; Coverage Contrast 1.
## Grunge Spots Dirty
Balance 0.5; Contrast 0; Invert False; Non Square Expansion True; Advanced: Coverage 0.5; Scale 1.
## Liquid
Scale 32; Disorder 0; Warp Intensity 0.5; Non Square Expansion True.
## Messy Fibers 1
Scale 1; Angle 0 (turns/degrees); Angle Random 0.5; Lines Number 16; Disorder 0; Non Square Expansion True.
## Messy Fibers 2
Same as Fibers 1 but "Links Number" 16 (chain-like links instead of lines).
## Messy Fibers 3
Scale 1; Angle 0; Angle Random 0.5; Color Random 0; Disorder 0; Non Square Expansion True.
## Grunge Map 004–009, 012–015
All share: Balance 0.5; Contrast 0.5; Invert False; Brush Pattern 0 (slider, selects the brush/stroke pattern variant); Non Square Expansion True.
## Grunge Map 010, 011
Same plus Cracks 0.5 (amount of crack-like detail), listed first.
## Grunge Rough Dirty (grunge_rough_dirty)
Balance 0.5; Contrast 0; Invert False; NSE True; Advanced: Main Grunge Intensity 0.5; Invert Scratches True; Scratches Intensity 0.2; Grain Intensity 1.
## Grunge Rust Fine
Balance 0.5; Contrast 0; Invert False; NSE True; Advanced: Base Grunge Contrast 0; Base warp Intensity 128; Streaks Intensity 0.5; Noise Intensity 0.5; Sharpen Intensity 0.5 (approx; slider low).
## Grunge Scratches Dirty
Balance/Contrast/Invert/NSE; Advanced: Base Grunge Intensity 0.25; Scratches Intensity 0.5.
## Grunge Scratches Fine
Balance/Contrast/Invert/NSE; Advanced: Scratches Amount 0.5; Sharpen Intensity 0.25; Scratch Value Bias 0.5.
## Grunge Scratches Rough
Balance/Contrast/Invert/NSE; Advanced: Scratch Quantity 1; Scratch Tiling 1; Scratch Blur 0; Scratch Width 0.5; Scratch Length 1; Scratch Masking 0.25; Scratch Dirtiness 0.5; Double Scratch 0.5; Scratch Spots Intensity 1; Scratch Spots Tiling 1; Dust Intensity 0.5; Dust Tiling 1; Sharpen Intensity 0.25.
## Grunge Shavings
Balance/Contrast/Invert/NSE; Advanced: Scratch Spots Amount 0.5; Scratch Spots Tiling 1; Dust Intensity 0.5; Sharpen Intensity 0.25.
## Clouds 3
Scale 1; Disorder 0; NSE True.
## Creased
Scale 1; Disorder 0; Warp Intensity 0.25; NSE True.
## Crystal 1
Scale 32; Disorder 0; NSE True.
## Crystal 2
Scale 1; Disorder 0; NSE True.
## Directional Noise 1
Scale 1; Angle 0 (turns/degrees); Angle Random 0; Disorder 0; NSE True.
## Directional Noise 2
Scale 1; Disorder 0; Angle 0; Angle Random 0; NSE True.
## Directional Noise 3, 4
Scale 1; Disorder 0; Angle 0; Angle Random 0; NSE True.
## Directional Scratches
Scale 4; Angle 0; Angle Random 0; Disorder 0; Pattern Amount 1; Pattern Size X 0.5 / Y 20; Pattern Size Random X/Y 0; NSE True.
## Dirt 1, Dirt 2, Dirt 3
Scale 1; Disorder 0; NSE True.
## Dirt 4, Dirt 5
Scale 1; Disorder 0; NSE True.
## Dirt Gradient
Disorder 0; NSE True (no scale).
## Fluid
Scale 1; Disorder 0; Warp Intensity 0.5; Pattern Size X 0.5 / Y 1; NSE True.
## Fractal Sum 1, 2
Only NSE True (no other instance parameters; fixed-look fractal sum).
## Fractal Sum 3, 4
Only NSE True.
## Fractal Sum Base (fractal_sum_base_2)
Roughness 0.5 (shown 1?); Global Opacity 1; Min Level 1; Max Level 11; NSE True. (Roughness slider mid, value text ~0.5 range; defaults as displayed: Roughness 0.5, Global Opacity 1.)
## Fur 1
Scale 2; Disorder 0; NSE True.
## Fur 2
Scale 1; Waves Scale 0; Waves Rotation 0.125; NSE True.
## Fur 3
Scale 1; Disorder 0; Waves Amount 2; NSE True.
## Gaussian Noise
Scale 32; Disorder 0; NSE True.
## Gaussian Spots 1, 2
Scale 1; Disorder 0; NSE True.
## Grunge Concrete
Balance 0.5; Contrast 0; Invert False; NSE True; Advanced: Base Noisiness 1; Dirt Specks Opacity 0.5; Invert Dirt False; Scratches Opacity 0.215; Sharpen 0.5; Large Variation Intensity 0.5.
## Grunge Damas
Balance 0.5; Contrast 0; Invert False; NSE True; Advanced: Distortion 1; Divisions 16; Waves 1; Details 0.5; Rotation Random 0.05.
## Grunge Galvanic Large
Balance 0.5; Contrast 0; Invert False; NSE True; Advanced: Warp Intensity 64; Ridge Detail Opacity 1; Sharpen Intensity 0.8.
## Grunge Galvanic Small
Balance 0.5; Contrast 0; Invert False; NSE True; Advanced: Crispness 0.5; Dirt 0.5; Micro Distortion 0.5.
## Grunge Leaks
Balance/Contrast/Invert/NSE; Advanced: Drip Length 64; Shape Contrast 0.5; Drip Crispness 0.5; Sharpen Intensity 1.
## Grunge Leaky Paint (Leaks Dirty)
Balance/Contrast/Invert/NSE; Advanced: Leak Intensity 0.5; Leak Scale 1; Leak Angle Random 0; Leak Crispness 0.9.
## Grunge Map 001, 002, 003
Balance 0.5; Contrast 0.5; Invert False; Brush Pattern 0; NSE True.
## 3D Perlin Noise
Invert False; Scale 32; Size XYZ 1; Offset XYZ 0; Distortion Intensity 0; Distortion Scale Multiplier 1; Baseline 0.5; Contrast 0; Absolute False; Enable Tiling False. Input: Position (3D coordinate map).
## 3D Perlin Noise Fractal
As 3D Perlin plus Min Level 1; Max Level 7; Roughness 0.5; Lacunarity 2; Global Opacity 1.
## 3D Ridged Noise Fractal
Invert False; Scale 32; Size XYZ; Offset XYZ; Distortion Intensity 0; Distortion Scale Multiplier 1; Min Level 1; Max Level 7; Roughness 0.5 (slider low); Lacunarity 2; Global Opacity 1; Baseline 0.5; Contrast 0; Enable Tiling False (no Absolute).
## 3D Simplex Noise
Scale 32; Size XYZ 1 only.
## 3D Voronoi
Invert False; Scale 8; Size XYZ 1; Offset XYZ 0; Disorder 1 (single); Distortion Intensity 0; Distortion Scale Multiplier 1; Rounded Curve 0; Distance Scale 1; Distance Mode {Euclidean*, Manhattan, Chebyshev, Minkowski}; Style {F1*, F2, F2-F1, F1*F2, F1/F2, Edge, Random Color}; Enable Tiling False.
## 3D Voronoi Fractal
3D Voronoi plus Min Level 0; Max Level 4; Roughness 0.5; Lacunarity 2; Global Opacity 1; Blend Mode {Add*, Max, Min}.
## 3D Worley Noise
Scale 32; Size XYZ 1; Mode {Euclidean*, Manhattan, Chebyshev, Minkowski}; Style {F1*, F2, F2-F1, Border, Random Color}; Roundess 0; Invert False. (No tiling option.)
## Anisotropic Noise
X Amount 4; Y Amount 256; Y Amount By Resolution False; Rotate False; Smoothness 1; Smoothness Interpolation 0.5; NSE True.
## Blue Noise Fast
Rotation 1 (only parameter).
## BnW Spots 1
Scale 1; Disorder 0; Roughness 1; NSE True.
## BnW Spots 2, 3
Scale 1; Disorder 0; NSE True.

## Cells 1-4 and Clouds 1-2 (added in the second pass)

## Cells 1 (cells_1)
Scale 32; Disorder 0; NSE True.
## Cells 2 (cells_2)
Scale 32; Edge Width 2; Invert False; Disorder 0; NSE True.
## Cells 3 (cells_3)
Scale 32; Hardness 0.5; Invert False; Disorder 0; NSE True.
## Cells 4 (cells_4)
Scale 32; Disorder 0; Color Source {Random*, Pseudo Random (manual seed), Image Input}; NSE True.
## Clouds 1 / Clouds 2
Scale 1; Disorder 0; NSE True (same as Clouds 3).

## Noise dropdown option lists (second pass)
- **3D Voronoi**: Distance Mode {Euclidean*, Manhattan, Chebyshev, Minkowski}; Style {F1*, F2, F2-F1, F1*F2, F1/F2, Edge, Random Color}.
- **3D Voronoi Fractal**: same two lists plus Blend Mode {Add*, Max, Min}.
- **3D Worley Noise**: Mode {Euclidean*, Manhattan, Chebyshev, Minkowski}; Style {F1*, F2, F2-F1, Border, Random Color}.
- Voronoi / Voronoi Fractal (2D): Distance Mode and Style same as 3D Voronoi (Voronoi Fractal Blend Mode {Add*, Max, Min} as 3D Voronoi Fractal).
- **RT Caustics (color)**: Output Color Space {Raw, sRGB*}; Photon Grid Size {Auto*, 512, 1024, 2048, 4096}. RT Caustics 2 (grayscale) same minus Dispersion/Background Color.
- **Cells 4**: Color Source {Random*, Pseudo Random (manual seed), Image Input}.
## Cells / Clouds (previously missing)
- Cells 1 (cells_1): Scale 32; Disorder 0; Non Square Expansion True.
- Cells 2 (cells_2): Scale 32; Edge Width 2; Invert False; Disorder 0; NSE True.
- Cells 3 (cells_3): Scale 32; Hardness 0.5; Invert False; Disorder 0; NSE True.
- Cells 4 (cells_4): Scale 32; Disorder 0; Color Source {Random*, Pseudo Random (manual seed), Image Input}; NSE True.
- Clouds 1 (clouds_1), Clouds 2 (clouds_2): Scale 1; Disorder 0; NSE True (same as Clouds 3).

## =====  PATTERNS (Texture generators > Patterns)  =====

Same conventions as above. `*` marks the default entry of a dropdown. Group names (bold) are the sub-headings Designer shows inside Instance Parameters. 'NSE' = Non Square Expansion.

Families: **Gradients** (3D Linear Gradient, Gradient Axial, Axial Reflected, Circular, Linear 1-3, Radial), **Shapes** (Shape, Shape Mapper, Shape Extrude, Polygon 1-2, Star, Starburst, Stripes, Panorama Shape, Gaussian 1-2, Cube 3D, Cube 3D GBuffers, 3D Volume Mask, Height Extrude), **Tilers** (Tile Generator, Tile Random 1/2, Tile Sampler and their Color variants, Brick Generator, Brick 1-2, Checker 1, Weave 1-2, Weave Generator, Alveolus, Arc Pavement, Mesh 1-2, Triangle Grid Color/Grayscale), **Scatterers** (Splatter, Splatter Color, Splatter Circular + Color, Shape Splatter, Shape Splatter Blend/Blend Color/Data Extract/To Mask), **Fibers/Scratches** (Fibers 1-2, Scratches Generator, Scratches Generator Normal).

## 3D Linear Gradient (3d_linear_gradient): Points Position Mode {UV Positions*, World Space Positions}; Point 1 X/Y 0.75/0.75; Point 2 X/Y 0.25/0.25; Contrast 0. (Uses position map input.)
## 3D Volume Mask (3d_volume_mask): Shape {Cube*, Cylinder, Sphere}; Scale 0.25; Size X/Y/Z 1/1/1; Position Input {UV Position*, World Space Position}; Position UV X/Y 0.5/0.5; Rotation X/Y/Z 0/0/0; Feather Width 0.5.
## Alveolus (alveolus): Tiling 1; Gradient Filled Cells True; Interstice Width 0; Non Square Expansion True.
## Arc Pavement (arc_pavement): Scale 1; Pattern Amount 12; Pattern Amount Random 0; Pattern Minimum Amount 1; Arcs Amount 14; Pattern {Image Input, Square*, Disc, Paraboloid, Bell, Gaussian, Thorn, Pyramid, Brick, Gradations, Waves, Half Bell, Ridged Bell, Crescent, Capsule, Cone, (maybe more)}; Pattern Scale 1; Pattern Width 0.8; Pattern Height 0.9; Pattern Width Random 0; Pattern Height Random 0; Global Pattern Width Random 0; Pattern Height Decrease 0.25; Color Random 0; Non Square Expansion True.
## Brick 1 (brick_1): Scale 4; Edge Smoothness 0; Interstice Width 0.1; Non Square Expansion True.
## Brick 2 (brick_2): Scale 2; Edge Smoothness 0; Interstice Width 0.1; Non Square Expansion True.
## Brick Generator (brick_generator) — flat list with composite (vector) params
Bricks X/Y 4/8; Bevel X/Y 0.5/0.5, Smoothness 0, Round Corners 0; Keep Ratio True; Gap X/Y 0/0; Middle Size X/Y 0.5/0.5; Height Min 1, Max 1, Balance 0, Variation 0; Slope X 0, Y 0, Balance 0, Variation 0; Offset 0.5, Variation 0; Non Square Expansion True.
## Checker 1 (checker_1): Tiling 8; Rotate 45 Degrees False; Non Square Expansion True.
## Cube 3D (cube_3d) / Cube 3D GBuffers (cube_3d_gbuffers) — identical params: Orientation Offset X/Y 0.25/0.25; Size X/Y/Z 1/1/1; Scale 0.25; Non Square Expansion True. (GBuffers variant outputs multiple G-buffer style outputs.)
## Fibers 1 (fibers_1) / Fibers 2 (fibers_2): Tiling 1; Non Square Expansion True.
## Gaussian 1 / Gaussian 2 (gaussian_1/2): Tiling 1; Non Square Expansion True.
## Gradient Axial (gradient_axial) / Gradient Axial Reflected (gradient_axial_reflected): Point 1 X/Y 0.25/0.25; Point 2 X/Y 0.75/0.75; Non Square Expansion True.
## Gradient Circular (gradient_circular): Point 1 X/Y 0.5/0.5; Point 2 X/Y 1/0.5; Angle (Turns/Degrees dial) 0; Non Square Expansion True.
## Gradient Linear 1 (gradient_linear_1) / 2: Tiling 1; Rotation {0°*, 90°, 180°, 270°}.
## Gradient Linear 3 (gradient_linear_3): Tiling 1; Position 0.5; Rotation {0*, 90, 180, 270}.
## Gradient Radial (gradient_radial): Shape {Cone*, Hemisphere}; Point 1 X/Y 0.5/0.5; Point 2 X/Y 0.8518/0.4213 (default); Non Square Expansion True.
## Height Extrude (height_extrude): Camera Angle X/Y 0.5/0.75; Camera Scale 0.707; Height Scale 1; Vertical Offset 0; Ground Off (toggle); Normal Format {OpenGL, DirectX*} (button pair); Normal Intensity 128.
## Mesh 1 (mesh_1): Tiling 2; Rotate 45 Degrees False; Non Square Expansion True.
## Mesh 2 (mesh_2): Tiling 1; Rotate 45 Degrees False; Non Square Expansion True.
## Panorama Shape  (Patterns)
Sections: Base Parameters, Attributes, Instance Parameters, Input Values.
Instance Parameters: group **Shape Matrix** (Matrix: Rotation Angle + buttons 90cw/180/90ccw; Stretch Width %/Height % with Apply; Hori Mirror, Vert Mirror, x2, /2; Edit Matrix Values; Offset X/Y 0), then Shape dropdown (square, disc; default square); Shape Color (colour picker, HSV, default V 230 = near white); Shape Intensity 1; Shape Soft Border 0.1; Hotspot Intensity 10; Hotspot Size 0.5; Hotspot Falloff 0.9; Hotspot Position X/Y 0.5/0.5; Enable Backgound (sic) False; Enable Texture Input False.
## Polygon 1
Sides 6; Explode 0; Triangle Size X/Y 1/1; Scale 1; Auto Scale True; Rotation (turns/degrees) 0; Gradient False; Gradient Invert False; Tiling 1; Non Square Expansion True; Non Square Tiling False.
## Polygon 2
Tiling 1; Sides 6; Scale 1; Rotation 0; Curve 0; Gradient 1; Invert Gradient False; Auto Scale True; Non Square Expansion True.
## Scratches Generator (scratches_generator)
Spline Number 256; Max Segments Per Spline 128; Spline Rotation 0.5 turns/180 deg; Spline Rotation Random 0.5; Spline Scale 0.2; Spline Scale Random 1; Spline Distortion 0; Spline Distortion Random 0.5; Spline Distortion Frequency 0.5; Spline Width 1; Spline Width Random 0.5; Spline Position Random 1; Set Spline Width in px False; Luminance Random 1; Fade Mode dropdown (None, Start, End, Start + End; default Start); Fade Length 1; Non Square Expansion True.
## Scratches Generator Normal (scratches_generator_normal)
Same spline controls (Spline Scale 0.75) but instead of Luminance Random: Normal Intensity 0.5; Normal Intensity Random 1; Normal Format dropdown (DirectX, OpenGL; default DirectX); then Fade Mode (None, Start, End, Start + End), Fade Length 1, NSE True.
## Shape
Tiling 1; Pattern dropdown (Square, Disc, Paraboloid, Bell, Gaussian, Thorn, Pyramid, Brick, Gradation, Waves, Half Bell, Ridged Bell, Crescent, Capsule, Cone, Hemisphere; default Square); Scale 1; Size X/Y 1/1; Angle 0; Rotation 45° False; Non Square Expansion True; Non Square Tiling False.
## Shape Extrude (shape_extrude_2)
Extrude Height 1; Extrude Depth 1; Extrude Shape dropdown (Cube, Cylinder, Custom Input; default Cylinder); Extrude Shape Size X/Y 1/1; Scale 0.75; Profile Type dropdown (Straight, Vertical Gradient, Mask; default Straight); Bevel Height 0.1; Bevel Intensity 0.25; Bevel Curve 0; Mirror Bevel False; Downscale Multiplier 0; Position X 0.75 / Y 0.25; Output Range dropdown ([0, 1], [-1, 1]; default [0, 1]).
## Shape Mapper
Shape dropdown (Circle, Polygon; default Circle); Pattern Amount 8; Radius 0.5; Width 0.5; Rotation 0; Flip one on two False; Non Square Expansion True.
## Shape Splatter (shape_splatter)  — scatter family; groups shown as collapsible sub-headings
Top level: X Amount 16; Y Amount 16.
Group **Pattern**: Pattern Input Number 1; Pattern Distribution Mode dropdown (Random, Pattern Index, Line Index, Column Index; default Random); Pattern Distribution Map Multiplier 0; Pattern Rotation dropdown (0, 90, 180, 270; default 0); Pattern Rotation Random 0.
Group **Size**: Scale 1; Scale Random 0; Scale No Overlap 0; Scale Map Multiplier 0; Size X/Y 1/1; Size Ratio from Bg Slope 0; Preserve Aspect Ratio 0.
Group **Position**: Position Random 0; Random Distribution dropdown (Gaussian, Uniform; default Gaussian); Vector Map Multiplier 0; Offset Horizontal 0; Offset Vertical 0; Out of Bounds Option dropdown (Scale Shape, Constrain Position; list may continue; default Scale Shape).
Group **Rotation**: Rotation 0 (turns/degrees); Rotation Random 0; Rotation from Bg Slope 0; Rotation Map Multiplier 0; Vector Map Multiplier 0.
Group **Height**: Height Scale Auto Adjust True; Height Offset 0; Height Offset Random 0; Height Offset Map Multiplier 0; Height Scale 1; Height Scale Random 0; Height Scale Map Multiplier 0; Conform to Background 0; Smooth Conformed Background 0; Skew from Bg Slope 0; Background Slope Smoothness 0; Cutout Black Pixels True; Flatten Pattern Base False.
Group **Masking** (collapsed by default): Mask Random 0; Mask Random Map Multiplier 0; Mask from Bg Slope 0.
Inputs: pattern images (Pattern Input Number selects), background, maps for the *Map Multiplier controls.
## Shape Splatter Blend (shape_splatter_blend) — no groups, Instance Parameters only
Pattern Number 1; Luminance Adjustment 0; Luminance Random 0; Color Input Opacity 0. (Companion to Shape Splatter: blends per-pattern luminance.)
## Shape Splatter Blend Color (shape_splatter_blend_color)
Pattern Number 1; Random Pattern Assignment 0; Is Normal Map False (toggle); HSL Adjustment (Hue/Saturation/Luminance, 0 each); HSL Random (Hue/Saturation/Luminance, 0 each); Color Input Opacity 0.
## Shape Splatter Data Extract (shape_splatter_data_extract) — no parameters (utility; extracts splatter data from Shape Splatter output)
## Shape Splatter To Mask (shape_splatter_to_mask)
Pattern ID Start Range 1; Pattern ID End Range 1; Random Mask 0; Output dropdown (default Binary Mask; other options not opened).
## Splatter (splatter) — flat list, no groups
Pattern Size Width 100; Pattern Size Height 100; Rotation 0; Rotation Variation 0; Zoom 100; Gain 1; Pan X 0; Pan Y 0; Disorder 0; Grid Number 4; Disorder Angle 0; Disorder Random False; Pattern Size 9; Size Variation 0; Image Input Filtering (Engine > v4 only) {Bilinear + Mipmaps*, Bilinear, Nearest}; Output Level Min 0; Output Level Max 1; Background Color (grayscale, 0); Luminance Variation 0.
## Splatter Circular (splatter_circular) / Splatter Circular Color (splatter_circular_color) — same layout; Color variant adds Color group
Top: Pattern Amount 10; Pattern Amount Random 0; Pattern Amount Random Min 1; Ring Amount 1; Non Square Expansion True.
Group **Pattern**: Pattern dropdown (Image Input, Square, Disc, Paraboloid*, Bell, Gaussian, Thorn, Pyramid, Brick, Gradation, ... list continues – same family as Panorama Shape list; default Paraboloid); Symmetry Random 0; Symmetry Random Mode {Horizontal + Vertical*, Horizontal, Vertical}.
Group **Position**: Radius 0.25; Radius Random 0; Ring Radius Multiplier 1; Angle Random 0; Spiral Factor 0; Spread 1; Offset along Direction 0; Global Offset X/Y 0/0.
Group **Size**: Connect Patterns False; Size X/Y 0.1/0.1; Size Random X/Y 0/0; Scale 1; Scale Random 0; Scale by Pattern Number 0; Invert Pattern Number False; Scale by Ring Number 0; Invert Ring Number False.
Group **Rotation**: Pattern Rotation (dial: Turns/Degrees) 0; Pattern Rotation Random 0; Pattern Rotation Pivot {Center*, Min X, Max X, Min Y, Max Y}; Center Orientation True; Ring Rotation (Turns/Degrees) 0; Ring Rotation Random 0; Ring Rotation Offset 0.
Group **Color** (Color variant only? shown in the Color node): Color (picker, HSV, default white 255); Color Random Hue/Saturation/Luminance/Alpha 0; Luminance By Scale 0; Luminance by Pattern Number 0; Invert Pattern Number False; Luminance by Ring Number 0; Invert Ring Number False; Random Mask 0; Background Color (default black, alpha 255); Blending Mode {Add, Alpha Blend*}; Global Opacity 1.
## Splatter Color (splatter_color): identical to Splatter, but Background Color is a full RGBA/HSV picker (default black, alpha 255) and the last param is Color Variation 0 instead of Luminance Variation. Image Input Filtering {Bilinear + Mipmaps*, Bilinear, Nearest}.
## Star (star): Scale 1; Non Square Expansion True.
## Starburst (starburst): Branches 20; Blur Amount 2; Randomness 0; Size Min 0.05; Size Max 1; Size Variation 0; Tiling 1.
## Stripes (stripes): Stripes 10; Width 0.5; Softness 0; Shift 10 (pos. slider); Align {Edges*, Center}; Filtering True; Non Square Expansion True.
## Tile Generator (tile_generator) — groups: Pattern, Size, Position, Rotation, Color
Top: X Amount 10; Y Amount 10; Non Square Expansion True.
**Pattern**: Pattern dropdown {Image Input, Square, Disc, Paraboloid, Bell, Gaussian, Thorn, Pyramid, Brick*, Gradation, Waves, Half Bell, Ridged Bell, Crescent, Capsule, Cone, Gradation w. offset}; Pattern Specific (value 0.2) + Variation 0 (sub-sliders; meaning depends on pattern); Rotation dropdown {0*, 90, 180, 270}; Rotation Random 0; Quincunx Flip False; Symmetry Random 0; Symmetry Random Mode {Horizontal + Vertical*, Horizontal, Vertical}.
**Size**: Size Mode {Normal - Interstice*, Normal - Size, Keep Ratio, Absolute, Pixel}; Middle Size Horizontal/Vertical 0.5/0.5; Interstice X/Y: Interstice X 0, Random 0, Interstice Y 0, Random 0; Scale 1; Scale Random 0; Scale Random Seed 0.
**Position**: Offset 0; Offset Random 0; Offset Random Seed 0; Vertical Offset False; Position Random X/Y 0/0; Global Offset X/Y 0/0.
**Rotation**: Rotation (Turns/Degrees dial) 0; Rotation Random 0.
**Color**: Color (grayscale, white 255); Luminance Random 0; Luminance By Number False; Luminance By Scale False; Checker Mask False; Horizontal Mask False; Vertical Mask False; Random Mask 0; Invert Mask False; Blending Mode {Add*, Max, Add Sub}; Background Color (gray 0); Global Opacity 1; Reverse Rendering Order False.
## Tile Generator Color (tile_generator_color): same groups/params as Tile Generator, but Color group uses RGBA/HSV pickers: Color (white), Color Random Hue/Saturation/Luminance/Alpha 0 (replaces Luminance Random), Luminance By Number False, Luminance By Scale False, masks as Tile Generator, Blending Mode {Add*,...}, Background Color RGBA, Global Opacity 1, Reverse Rendering Order False; plus By Number Parametrisation (default Luminance) and By Scale Parametrisation (default Luminance) dropdowns (options not opened).
## Tile Random (tile_random) — groups: Pattern, Split, Size, Interstice, Shape, Position, Color, Mask
Top: X Amount 4; Y Amount 4; Non Square Expansion True.
**Pattern**: Pattern {Pattern Input, Square*, Disc, Paraboloid, Bell, Gaussian, Thorn, Pyramid, Brick, Gradation, Waves, Half Bell, Ridged Bell, Crescent, Capsule, Cone, Gradation w. offset}; Rotation {0*,90,180,270}; Rotation Random 0; Symmetry Random 0; Symmetry Random Mode {Horizontal + Vertical*, Horizontal, Vertical}.
**Split**: Mode {none, auto*, auto horizontal, auto vertical, random h+v}; Threshold 0.5; Multiplier 0.
**Size**: Random X 0.5; Random Y 0.5.
**Interstice**: Mode {Relative to smallest brick, Relative to largest brick*}; Amount 0.2.
**Shape**: Scale 1; Scale Random 0; Rotation (Turns/Degrees) 0; Rotation Random 0; Rotation Constraint True.
**Position**: Offset 0; Offset Random 0; Random 0; Random Constraints True.
**Color**: Color (gray, white 255); Color Random 0; Color Parametrisation {None*, Area, Size X, Size Y}; Background Color (gray 0); Blending Mode {Add/Sub*, Max}.
**Mask**: Random 0; Invert False.
## Tile Random 2 (tile_random_2) — groups: Size, Shape Aspect - Transform, Slant, Bevel, Mask
Top: Amount X 10; Amount Y 10.
**Size**: Random Size Multiplier 1; Random Size Input Map Multiplier 0; Random Size X 1; Random Size Y 1; Random Size Distribution {Uniform*, Blue Noise}.
**Shape Aspect - Transform**: Interstice Thickness 0; Random Position Multiplier 0; Round Corners Radius 0; Round Corners Radius Input Map Mult. 0; Scale Multiplier 1; Scale Random 0; Rotation 0; Rotation Random 0; Corners Position Random 0.
**Slant**: Random Slant Multiplier 0; Random Slant Input Map Multiplier 0; Random Slant X 1; Random Slant Y 1; Random Slant Distribution {Uniform*, Blue Noise}.
**Bevel**: Bevel Distance Mode {Relative to Grid Size*, Relative to Shape Size, Relative to Image Size}; Bevel Distance Multiplier 0.1; Bevel Distance Input Map Mult. 0; Bevel Rounded Curve 0; Enable per Axis Bevel Control False.
**Mask**: Mask Random Invert False; Mask Random Start 0; Mask Random End 0; Mask by Cell Area Invert False; Mask by Cell Area Start 0; Mask by Cell Area End 0; Mask Map Input Invert False; Mask Map Input Start 0; Mask Map Input End 0.
## Tile Random Color (tile_random_color): as Tile Random but Color group uses RGBA/HSV: Color (white); Color Random Hue/Saturation/Luminance/Opacity 0; Color Parametrisation {None*, Area, Size X, Size Y}; Color Parametrization Affect {RGB+Alpha, RGB only*, Alpha only}; Background Color RGBA (black); Blending Mode {Add/Sub*, Max}. Mask group: Random 0, Invert False.
## Tile Sampler (tile_sampler) — groups: Pattern, Size, Position, Rotation, Color (the most complete tiler; supports input maps)
Top: X Amount 16; Y Amount 16; Non Square Expansion True.
**Pattern**: Pattern {Pattern Input, Square*, Disc, Paraboloid, Bell, Gaussian, Thorn, Pyramid, Brick, Gradation, Waves, Half Bell, Ridged Bell, Crescent, Capsule, Cone, Gradation w. offset}; Rotation {0*,90,180,270}; Rotation Random 0; Symmetry Random 0; Symmetry Random Mode {Horizontal + Vertical*, Horizontal, Vertical}.
**Size**: Size Mode {Normal*, Keep Ratio, Absolute, Pixel}; Size X/Y 1/1; Size Random X/Y 0/0; Scale 0.8; Scale Random 0; Scale Map Multiplier 0; Scale Vector Map Multiplier 0; Scale Map Parametrization Affect {X and Y*, X, Y}.
**Position**: Position Random 0; Offset 0; Offset Type {horizontal quincux*, vertical quincux, horizontal global, vertical global}; Global Offset X/Y 0/0; Displacement Map Intensity 0; Displacement Angle (Turns/Degrees dial) 0; Vector Map Displacement 0.
**Rotation**: Rotation (Turns/Degrees dial) 0; Rotation Random 0; Rotation Map Multiplier 0; Vector Map Multiplier 0.
**Color**: Mask Map Threshold 0; Mask Map Invert False; Mask Map Sampling Technique {Pattern Center*, Pattern Bounding Box (slower)}; Mask Random 0; Invert Mask False; Blending Mode {Add/Sub, Max*}; Color (gray white 255); Color Random 0; Color Parametrization Mode {Color Input*, Scale, Row Index, Column Index, Pattern Index}; Color Parametrization Multiplier 0; Global Opacity 1; Background Color (gray 0); Reverse Rendering Order False.
## Tile Sampler Color (tile_sampler_color): same groups/params as Tile Sampler; Color group differences: Blending Mode {Add/Sub, Alpha Blend*}; Color is RGBA/HSV picker; Color Random Hue/Saturation/Luminance/Opacity 0; Color Parametrization Mode {Color Map*, Scale, Row Index, Column Index, Pattern Index, Pattern Center Position, Pattern Center Position (RG) Bsphere Size (B)}; Color Parametrization Multiplier 0; Color Parametrization Affect {RGB+Alpha*, ... (RGB only, Alpha only presumably)}; Background Color RGBA (black); Reverse Rendering Order False. Mask Map Sampling Technique {Pattern Center*, Pattern Bounding Box (slower)}.
## Triangle Grid Color (triangle_grid_color) / Triangle Grid Grayscale (triangle_grid_grayscale) — flat list, same layout
Color Output {Per Vertex*, Per Triangle, Thin Line, Distance to Edge, Center}; Triangulation {Auto*, 45°, -45°, Quincux Horizontal, Quincux Vertical}; X Amount 16; Y Amount 16; Random Position Multiplier 1 (slider 0..?, mid-position); Random Position X/Y 1/1; Vector Map Displacement 0; Quincux Offset X 0; Quincux Offset Y 0; Rotation (Turns/Degrees dial) 0; Rotation Disorder 0.5; Height Input Multiplier 0; Height Random 1; Blend Mode {Max*, Depth Test}; Color Source {Height, Color Input, Random Grayscale*, Random Color} (Color node) | {Height, Color Input, Random*} (Grayscale node); Random Color Seed Mode {Global Random Seed*, Manual Seed} (Manual Seed adds a seed field); Non Square Expansion True.
## Weave 1 (weave_1): Tiling 1; Rotate 45 Degrees False; Non Square Expansion True.
## Weave 2 (weave_2): Scale 8; Disorder 0; Rotate 45 Degrees False; Non Square Expansion True.
## Weave Generator (weave_generator): Tile X 1; Tile Y 1; Shape 0.5; Weave 2; Gap X/Y 0.1/-0.1 (shown 0.1 and -0.1); Non Square Expansion True.

## COMMON SECTIONS (every node; shown when expanded)
**Base Parameters** (each row has a mode icon: Relative to Parent / Absolute [Inherit]; values greyed while inherited): Output Size (Width/Height, log2 steps, "Parent x 1", link toggle); Output Format {8 Bits per Channel, 16 Bits per Channel, HDR Low Precision (16F), HDR High Precision (32F)}; Pixel Size (Width/Height 1); Pixel Ratio {Stretch, Square*}; Tiling Mode {No Tiling, Horizontal Tiling, Vertical Tiling, H and V Tiling*}; Random Seed (0 + randomize button).
**Attributes**: Package (path, folder button); Graph instance (pkg:///<name>?dependency=<id>); Description (text); Category (e.g. Generator); Author (Adobe); Author URL; Tags (e.g. Pattern).
**Instance Parameters** (node-specific; may have collapsible sub-groups with the headings listed per node). **Input Values** (lists unconnected-input defaults, bottom of panel).

## Not captured (to check later)

- **Noises:** per-parameter slider ranges (only defaults were read); what Waveform 1's Pattern number (default 6) and Grunge Map Brush Pattern select; Fractal Sum 1-4 show only Non Square Expansion; Voronoi Minkowski exponent control (if any).
- **Patterns:** Splatter Circular (plain) was not opened separately from the Color variant; Shape Splatter's Out of Bounds Option list may have more than the two entries seen (Scale Shape, Constrain Position); Shape Splatter To Mask Output list (only the default Binary Mask was read); Tile Generator Color By Number / By Scale Parametrisation lists; Tile Sampler Color's Color Parametrization Affect list (assumed same as Tile Random Color); Tile Random 1 Color group's exact slider ranges; Panorama Shape and Polygon 1/2 group headings (read as flat lists).
- Input slots of each node (names and types of image inputs) were not recorded except where obvious.
- Base Parameters were read once on Gradient Radial (they are identical on every node); per-node differences in what is inherited were not checked.
- Atomic nodes, Filters, Material filters, Mesh-based generators, Spline & Path tools and Functions were not surveyed.

## MaterialCraft to-do

Noises status checked against `src/sbs_adapt.rs` and `src/sbs_library.txt` on 2026-10-20:

- Every noise listed in the first pass except **Blue Noise Fast** has an adapter that maps its parameters onto a MaterialCraft engine node (78 of 79 checked). Blue Noise Fast is listed in the library but has no adapter yet. Cells 1-4 and Clouds 1-2 were added to this survey afterwards and still need the same check.
- An adapter existing does not mean the look matches Designer. None of them has been checked pixel for pixel against these defaults; the Crystal 2 streaks and the Blur and Warp unit factors are known open items.
- Patterns have not been compared against MaterialCraft at all yet.

Checklist (each item: render the node in MaterialCraft at the defaults above and compare with Designer at 2048 px):

- [ ] Property panel: implement the four sections (Base Parameters, Attributes, Instance Parameters, Input Values) with the same order, the Relative to Parent / Absolute mode icon, and the same option lists for Output Format, Pixel Ratio and Tiling Mode.
- [ ] Property panel: support named collapsible sub-groups inside Instance Parameters, compound controls (XY / XYZ, angle dial with Turns and Degrees, colour picker with sRGB/Float and HSV) and two-button choices.
- [ ] Add an adapter for Blue Noise Fast (single parameter: Rotation 1).
- [ ] Voronoi / Voronoi Fractal / 3D Voronoi / 3D Worley: all Styles and Distance Modes (including Minkowski), Blend Mode {Add, Max, Min}, Rounded Curve, Distortion Intensity and Scale Multiplier, Offset, per-axis Disorder and Size.
- [ ] 3D noises: Position input, Enable Tiling, Absolute, Baseline, Contrast, 3D Worley Mode/Style/Roundess.
- [ ] Grunge nodes: every Advanced parameter listed above, in particular Scratches Rough (13 controls), Damas (Divisions, Waves, Rotation Random), Concrete (Dirt Specks, Scratches Opacity).
- [ ] Grunge Map 001-015: Brush Pattern and (010, 011) Cracks.
- [ ] Cells 1-4: Edge Width (Cells 2), Hardness (Cells 3), Color Source {Random, Pseudo Random (manual seed), Image Input} (Cells 4).
- [ ] Anisotropic Noise: Y Amount By Resolution, Rotate, Smoothness Interpolation.
- [ ] Directional Noise 1-4 and Directional Scratches: Angle Random, Pattern Size and Pattern Size Random.
- [ ] Fur 1-3: Waves Scale / Rotation / Amount; Messy Fibers 1-3 differences (Lines vs Links Number, Color Random).
- [ ] RT Caustics 1/2: Photon Size, Dispersion, Jittering, Light Position, Background Color, Output Color Space {Raw, sRGB}, Photon Grid Size {Auto, 512, 1024, 2048, 4096}.
- [ ] Fractal Sum Base: Roughness, Global Opacity, Min/Max Level (defaults 1 and 11).
- [ ] Moisture Noise, Fluid, Liquid, Creased, Microscope View, Plasma, Waveform 1: parameter parity.
- [ ] Tile Generator / Tile Sampler / Tile Random 1-2 and Color variants: all groups, Size Mode, Offset Type, Split Mode, Slant, Bevel (Tile Random 2), Mask groups, Blending Modes, Color Parametrization Mode, Reverse Rendering Order.
- [ ] Splatter family: Splatter, Splatter Color (Image Input Filtering), Splatter Circular (rings, spiral, pivot), Shape Splatter (six groups, Distribution Mode, Out of Bounds) and the Blend / Blend Color / To Mask / Data Extract companions.
- [ ] Simple patterns: Brick 1-2, Brick Generator, Checker 1, Weave 1-2, Weave Generator, Alveolus, Arc Pavement, Mesh 1-2, Triangle Grid Color/Grayscale, Star, Starburst, Stripes, Gaussian 1-2, Fibers 1-2, gradients, Height Extrude (Normal Format, Normal Intensity), Cube 3D, 3D Volume Mask, Panorama Shape, Polygon 1-2, Shape, Shape Mapper, Shape Extrude, Scratches Generator (+ Normal; Fade Mode).
- [ ] Fix the remaining items in "Not captured".
