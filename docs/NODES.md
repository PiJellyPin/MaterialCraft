# MaterialCraft node reference

217 texture node types, 84 function-graph node types and 5 pattern-program node types. This page is generated from the node definitions in `src/graph.rs` (`materialcraft nodes` prints the same data). Parameters are listed after the colon; every parameter has a default, and the editor's Properties panel shows ranges and units.

Common to most nodes: **Output Size** (relative to input, relative to parent or absolute, with a power-of-two modifier), **Output format** (graph default, 8 / 16-bit integer, 16-bit float or 32-bit float; the graph default is set under Node Precision and is 32-bit float unless changed), `seed`, `contrast`, `invert` on generators, and the shared **disorder** group (`disorder`, `disorder_speed`, `anisotropy`, `anisotropy_angle`) and **tile offset** (`tile_x`, `tile_y`) on noises. Every node tiles seamlessly and works on non-square graphs; the cell-based generators (`perlin`, `fbm`, `clouds`, `voronoi`, `cells`, `tile_sampler`, `tile_generator`) also take `square_cells=1` to keep their cells square when the graph is not.

Nodes with several outputs: `path_sample` (position, width, height, side); `shape_extrude` (height, walls); `uber_emboss` (emboss, highlight, shadow); `snow_filter` (height, mask); `water_filter` (color, mask, height, roughness); `color_match` (output, mask); `quantize_color` (output, id, palette); `weave_generator` (height, mask_1, mask_2, opacity); `rgba_split` (r, g, b, a); `alpha_split` (rgb, alpha); `voronoi` (output, cell_id, edge, distance); `cells` (output, cell_id, distance, edge); `splatter` (output, id, center, size, luminance); `flood_fill_to_position` (position, x, y); `flood_fill_to_bbox_size` (size, width, height); `frequency_split` (low, high); `bent_normal` (normal, ao); `convex_concave` (convex, concave); `paving` (height, cell_id, gaps); `metal_rough_to_spec_gloss` (diffuse, specular, glossiness); `spec_gloss_to_metal_rough` (base_color, metallic, roughness); `material_blend` (base_color, normal, height, roughness, metallic, ao); `path_border` (height, mask); `path_track` (height, rails, sleepers); `path_cable` (height, mask, strand); `path_seam` (height, stitches). Connect the desired output socket to an input socket, or to an Output node to expose it as a graph output.

## Generators

| Node | Inputs | Description and parameters |
|---|---|---|
| `color` |  | constant colour: r g b a |
| `white_noise` |  | per-pixel random: seed distribution(0 uniform,1 gaussian,2 triangle) |
| `gaussian_noise` |  | gaussian-distributed per-pixel noise: seed scale contrast invert disorder disorder_speed anisotropy anisotropy_angle tile_x tile_y |
| `perlin` |  | perlin noise: scale scale_x scale_y seed disorder disorder_speed tile_x tile_y |
| `fbm` |  | fractal noise/clouds: scale octaves gain seed mode(0 fbm,1 ridged,2 turbulence) disorder disorder_speed anisotropy anisotropy_angle tile_x tile_y |
| `voronoi` |  | cellular: scale jitter seed mode(0 f1,1 cell id,2 edge,3 f2,4 f2-f1,5 f1*f2,6 f1/f2) aspect offset_x offset_y distortion distortion_scale distance_mode(0 euclid,1 manhattan,2 chebyshev,3 minkowski) minkowski edge_thickness rounded distance_scale color_seed_mode color_seed invert |
| `voronoi_fractal` |  | layered voronoi: scale jitter seed mode aspect offset_x offset_y distortion distortion_scale distance_mode minkowski edge_thickness rounded distance_scale min_level max_level roughness lacunarity opacity blend(0 add,1 max,2 min) color_seed_mode color_seed invert |
| `gradient` |  | gradient: mode(0 linear,1 radial) angle |
| `circle` |  | radius softness |
| `rect` |  | width height softness |
| `polygon` |  | sides radius softness rotation |
| `checker` |  | tiles |
| `bricks` |  | rows cols mortar bevel offset lum_var seed |
| `pixel_processor` | in, in2, in3, in4 | per-pixel expr / expr_g / expr_b / expr_a (string params), or fn="function" to run a function graph per pixel |
| `shape` |  | procedural shape: square, disc, paraboloid, gaussian, diamond, pyramid, half sphere, cone |
| `tile_sampler` | pattern, pattern2, patte | rn3,pattern4,pattern5,pattern6,mask,scale_map,rotation_map,displacement_map,color_map,distribution_map,background grid of pattern instances: x_amount y_amount size aspect size_mode scale scale_random size_random rotation rotation_random pattern_rotation symmetry_random position_random offset offset_mode offset_random global_offset_x global_offset_y color color_random lum_by_number lum_by_scale mask_threshold mask_invert mask_random mask_checker invert_mask blending opacity background reverse_order pattern_type pattern_specific pattern_distribution |
| `tile_generator` | pattern, pattern2, patte | rn3,pattern4,pattern5,pattern6,mask,scale_map,rotation_map,displacement_map,color_map,distribution_map,background brick-style tiles: x_amount y_amount size aspect size_mode scale scale_random size_random rotation rotation_random pattern_rotation symmetry_random position_random offset offset_mode offset_random global_offset_x global_offset_y color color_random lum_by_number lum_by_scale mask_checker mask_horizontal mask_vertical mask_random invert_mask blending opacity background reverse_order pattern_type pattern_specific pattern_distribution |
| `splatter` | pattern, pattern2, patte | rn3,pattern4,pattern5,pattern6 random scatter of a pattern: count grid_number scale scale_var aspect rotation rot_var pattern_rotation lum_var disorder pan_x pan_y symmetry_random gain output_min output_max background blending opacity pattern_type pattern_specific pattern_distribution |
| `splatter_circular` | pattern, pattern2, patte | rn3,pattern4,pattern5,pattern6 instances on one or more circles: number amount_random rings radius radius_random ring_radius_multiplier angle_random spiral_factor spread size size_random scale_by_number scale_by_ring rotation rotation_random center_orientation ring_rotation ring_rotation_random position_random color luminance_random lum_by_number lum_by_ring mask_random background blending opacity pattern_type pattern_specific pattern_distribution |
| `clouds` |  | soft cloud noise: scale seed contrast invert disorder disorder_speed anisotropy anisotropy_angle tile_x tile_y |
| `bnw_spots` |  | black and white spots: scale seed contrast invert variant disorder disorder_speed anisotropy anisotropy_angle tile_x tile_y |
| `crystal` |  | faceted crystal cells: scale disorder seed contrast invert variant |
| `directional_noise` |  | streak noise, seamless at any angle: scale stretch angle seed contrast invert disorder disorder_speed anisotropy anisotropy_angle tile_x tile_y |
| `grunge` |  | grunge masks: variant(0 concrete,1 scratches,2 spots,3 smudges) balance large_variation base_noisiness dirt_specks_opacity invert_dirt scratches_opacity sharpen quantity scratch_width scratch_length details coverage coverage_contrast disorder disorder_speed seed contrast invert |
| `plasma` |  | smooth plasma waves |
| `stripes` |  | stripes: count angle softness width tile(1 = snap to seamless angle, 0 = exact) |
| `weave` |  | woven threads: count width |
| `star` |  | star shape: points inner size softness rotation |
| `gradient_axial` |  | symmetric gradient: angle |
| `gradient_circular` |  | angular gradient: offset |
| `cells` | color | cellular noise: scale jitter seed variant(0 cells1,1 cells2,2 cells3,3 cells4) luminance_random color_source(0 random,1 pseudorandom,2 image) pseudorandom_seed edge_width hardness contrast invert disorder disorder_speed anisotropy anisotropy_angle tile_x tile_y |
| `fractal_sum` |  | sum of noise octaves: scale min_level max_level roughness seed mode(0 base,1 turbulence,2 creased) contrast invert disorder disorder_speed |
| `waveform` |  | tileable wave pattern: count angle warp warp_scale seed shape(0 sine,1 triangle,2 saw,3 square) contrast invert disorder disorder_speed |
| `gaussian_spots` |  | soft gaussian spots: scale size size_variation seed contrast invert disorder disorder_speed anisotropy anisotropy_angle tile_x tile_y |
| `triangle_grid` |  | grid of triangles: style(0 squares,1 jittered mesh) count border variation x_amount y_amount random_position_multiplier quincunx_offset_x quincunx_offset_y triangulation(0 fixed,1 alternate,2 random) output(0 smooth,1 flat,2 edge distance) height_random thin_lines distance_to_edge_thickness thin_line_color thin_line_background seed contrast invert |
| `grunge_map` |  | grunge map 1-24: map balance contrast brush_pattern cracks disorder coverage seed invert |
| `weave_generator` |  | woven threads with gaps: tile_x tile_y weave shape gap_x gap_y contrast invert |
| `starburst` |  | star-like rays: count rays sharpness size rotation rotation_random seed contrast invert |
| `gradient_reflected` |  | gradient that goes 0-1-0: count angle contrast invert |
| `fx_map` | in, in2, in3, in4 | run a pattern program: program="name" depth seed blend (0 max 1 add 2 over 3 min 4 multiply) color bg bg_r bg_g bg_b |
| `svg` |  | vector image: file="art.svg" fit (0 contain 1 stretch 2 cover) scale x y |
| `text` |  | text: text="..." font="face.ttf" (optional) or font_family="Arial" (an installed family; a metric-compatible stand-in such as Liberation Sans when it is not installed, else the built-in face) size x y align (0 left 1 centre 2 right) spacing line_spacing rotation value wrap(width as a fraction of the image, 0 off) use_color color_r color_g color_b |
| `value_processor` | in, in2, in3, in4 | run fn="function" once; the result (float, float3 or float4) fills the image. The function can read image statistics |
| `blue_noise` |  | evenly spread grain with little low-frequency content: scale seed tightness contrast invert |
| `fibers` |  | random curved hair-like strands: count length thickness angle angle_variation curl taper brightness_variation seed contrast invert |
| `scratches` |  | random thin scratches: count length thickness angle angle_variation curl taper brightness_variation seed contrast invert |
| `caustics` |  | light patterns of water: scale octaves strength sharpness seed contrast invert |
| `liquid_noise` |  | swirling warped noise: scale octaves warp mode(0 liquid,1 marble) bands seed contrast invert |
| `perlin_3d` |  | slice of 3D noise that loops in depth: scale depth octaves gain seed contrast invert |
| `paving` |  | irregular paving stones (outputs: height, cell_id, gaps): scale jitter gap bevel height_variation dome seed contrast invert |
| `wire_mesh` |  | woven or chain-link wire: count_x count_y wire_width weave_depth mode(0 square,1 diamond) contrast invert |
| `path_stroke` |  | line along a spline (dashes, taper, profile 0 flat,1 round,2 bevel): file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit width softness profile taper_start taper_end dash_length dash_gap contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse |
| `path_fill` |  | inside of closed splines: file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit even_odd softness contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse |
| `path_distance` |  | distance to a spline: mode(0 falloff from the line,1 signed,2 inward gradient,3 outward gradient) range file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse |
| `path_border` |  | border or bevel along closed splines (outputs: height, mask): width side(0 inside,1 outside,2 both) profile(0 linear,1 round,2 smooth) flat file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse |
| `path_scatter` | in,mask | copies of a pattern along a spline: spacing scale scale_variation jitter_along jitter_across rotation(0 follow,1 fixed,2 random) angle angle_variation density opacity_variation blend(0 max,1 add) offset seed file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse |
| `path_track` |  | railway track along a spline (outputs: height, rails, sleepers): gauge rail_width sleeper_spacing sleeper_length sleeper_width rail_height sleeper_height file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse |
| `path_cable` |  | twisted cable along a spline (outputs: height, mask, strand): width strands strand_size twist file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse |
| `path_seam` |  | stitched seam along a spline (outputs: height, stitches): stitch_length stitch_gap stitch_width groove_width groove_depth stitch_height file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse |
| `path_splatter` | in,mask | several rows of copies of a pattern along a spline, thinned out by a mask: rows row_spacing spacing scale scale_variation jitter_along jitter_across rotation(0 follow,1 fixed,2 random) angle angle_variation density opacity_variation blend(0 max,1 add) offset seed file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse |
| `path_sample` |  | maps from a spline (outputs: position, width, height, side): width file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse contrast invert |
| `herringbone` |  | herringbone pavement: scale gap bevel variation mode(0 height,1 brick id,2 orientation) seed contrast invert |
| `arc_pavement` |  | fan-shaped arc pavement: scale rings stones gap bevel variation mode(0 height,1 stone id) seed contrast invert |
| `anisotropic_noise` |  | noise stretched along any direction, still tiling: scale stretch angle octaves gain seed contrast invert |
| `fur_noise` |  | fur-like hairs: count length thickness angle angle_variation curl taper root_shade brightness_variation seed contrast invert |
| `moisture_noise` |  | damp patches with drips: scale coverage softness drips drip_length seed contrast invert |
| `microscope_noise` |  | cells seen through a microscope: scale jitter membrane nucleus grain mode(0 combined,1 membrane,2 nuclei,3 cell id) seed contrast invert |
| `worley_3d` |  | cellular noise in 3D, one slice per z: scale z jitter mode(0 f1,1 f2,2 f2-f1,3 cell id) seed contrast invert |
| `fbm_3d` |  | fractal noise in 3D, one slice per z: scale z octaves gain mode(0 fbm,1 ridged,2 turbulence) seed contrast invert |
| `volume_mask` |  | slice through a repeated 3D shape as a mask: shape(0 sphere,1 box,2 cylinder,3 torus,4 cone) size repeat z roundness softness contrast invert |
| `gradient_3d` |  | distance gradient of a repeated 3D shape slice: shape size repeat z roundness range contrast invert |

## SDF

| Node | Inputs | Description and parameters |
|---|---|---|
| `sdf_shape` |  | 2D signed distance shape: shape(0 circle,1 box,2 polygon,3 ring) size aspect sides rotation round x y range |
| `sdf_op` | a, b | combine SDFs: op(0 union,1 subtract,2 intersect) smooth range |
| `sdf_mask` | in | SDF -> mask: offset softness |
| `sdf_form` |  | procedural 3D SDF form as height: shape(0 sphere,1 box,2 torus,3 cylinder) size repeat round scatter seed height |

## Utility

| Node | Inputs | Description and parameters |
|---|---|---|
| `bitmap` |  | flip_x flip_y gray; imported bitmap (PNG, JPEG, TIFF, EXR, HDR, WebP, DDS, TGA, BMP): file="image.png" |
| `baked_map` |  | map baked from meshes and stored in the package: set="name" map="normal" flip_y invert gray; see the mesh baker |
| `input` |  | subgraph input: idx |
| `subgraph` | in0, in1, in2, in3 | instance of a reusable subgraph: graph="name" |
| `output` | in | graph output: usage identifier |
| `dot` | in | re-route a link |
| `input_value` |  | graph parameter read by functions: name, type (0 float, 1 int, 2 bool, 3 float2, 4 float3, 5 float4, 6 int2, 7 int3, 8 int4, 9 string), v / x y z w, min max, label |

## Filters

| Node | Inputs | Description and parameters |
|---|---|---|
| `switch` | a, b, c, d | pass one of up to four inputs: index |
| `flood_fill` | in | label connected regions of a mask |
| `flood_fill_to_index` | in | region number of each flood-fill region as a gray level |
| `flood_fill_to_position` | in | centre of each flood-fill region (outputs: position, x, y) |
| `flood_fill_to_bbox_size` | in | bounding-box size of each flood-fill region (outputs: size, width, height) |
| `flood_fill_random` | in | random grayscale per flood-fill region |
| `flood_fill_color` | in | random color per flood-fill region |
| `flood_fill_gradient` | in | gradient across each flood-fill region |
| `vector_warp` | in, vector | displace by a vector map: intensity directx |
| `slope_blur` | in, slope | blur along a slope map: samples intensity mode |
| `blur_hq` | in | high quality blur: intensity quality |
| `histogram_range` | in | select a range of values: position range contrast invert |
| `bevel` | in | bevel a shape: distance smoothing threshold use_as_normal normal_intensity corner(0 round,1 angular) directx |
| `distance` | in | distance field of a mask: distance mode |
| `color_match` | in, reference, src_color | ,target_color recolour: method(0 statistics transfer from reference,1 colour swap) strength source_mode(0 average,1 colour,2 input) src_r src_g src_b target_mode(0 colour,1 input) target_r target_g target_b custom_variation variation_hue variation_chroma variation_luma use_mask mask_hue_range mask_chroma_range mask_luma_range mask_smoothness |
| `make_it_tile_photo` | in | make a photo tile: width width_v sharp sharp_v warp warp_v seed |
| `make_it_tile_patch` | in | make an image tile with random patches: mode(0 cells,1 stamped) patches softness octave mask_size mask_precision mask_warp size_x size_y disorder size_var rotation rotation_var bg_r bg_g bg_b bg_a color_var lum_var seed |
| `clamp` | in | clamp values: low high apply_alpha |
| `highpass` | in | high-pass filter: radius |
| `swirl` | in | swirl distortion: amount radius |
| `blur` | in | gaussian blur: radius |
| `directional_blur` | in | angle length |
| `sharpen` | in | radius amount |
| `levels` | in | low high gamma out_low out_high |
| `adjust` | in | brightness contrast |
| `invert` | in | invert colours: invert |
| `threshold` | in | binary threshold: threshold softness mode(0 greater,1 greater or equal,2 lower,3 lower or equal) |
| `histogram_scan` | in, position_map, contrast_map | position contrast invert; position_map / contrast_map (grayscale) give the position and contrast per pixel while use_position_map / use_contrast_map are on (Histogram Scan Non-Uniform) |
| `quantize` | in | steps |
| `to_gray` | in | luminance |
| `channel` | in | ch(0 r,1 g,2 b,3 a) |
| `merge` | r, g, b, a | combine channels |
| `tint` | in | multiply by colour: r g b |
| `colorize` | in | gradient map: interp(0 linear,1 smooth,2 step,3 smoother) smoothness addressing(0 clamp,1 repeat) use_alpha  stops p0 r0 g0 b0 a0 ... p15 r15 g15 b15 a15 |
| `hsl` | in | hue sat val |
| `transform` | in | offset_x offset_y scale_x scale_y rotation(turns) |
| `mirror` | in | mirror an image: axis(0 x,1 y,2 both,3 corner) offset_x offset_y invert_x invert_y corner(0 top left,1 top right,2 bottom left,3 bottom right) |
| `warp` | in, warp | slope warp: intensity filtering(0 bilinear,1 nearest,2 bicubic) |
| `directional_warp` | in, warp | warp in one direction: angle intensity offset filtering(0 bilinear,1 nearest,2 bicubic) |
| `blend` | fg, bg, mask | blend two images: mode(0 copy,1 add,2 sub,3 mul,4 screen,5 overlay,6 max,7 min,8 diff,9 softlight,20 add sub,21 switch) opacity alpha_mode(0 source alpha,1 ignore alpha,2 straight,3 premultiplied) crop_x0 crop_y0 crop_x1 crop_y1 |
| `quantize_color` | in | reduce to few colours (k-means): color_amount contour_smoothing dithering dithering_pattern(0 noise,1 bayer) ignore_alpha distance_color_space(0 lab,1 rgb) |
| `edge_detect` | in | edge lines: mode(0 lines,1 sobel) edge_width edge_roundness invert tolerance intensity |
| `emboss` | in, height | mode (0 relief of the input, 1 light the input with the height map, as Substance Designer's Emboss) angle intensity hl_r hl_g hl_b (highlight colour) sh_r sh_g sh_b (shadow colour) gloss (specular highlight, Emboss With Gloss) |
| `non_uniform_blur` | in, mask | blur by a mask: intensity |
| `radial_blur` | in | spin blur around a centre: mode(0 spin,1 zoom) samples angle x y amount radius |
| `anisotropic_blur` | in | blur stretched in one direction: intensity anisotropy angle |
| `symmetry` | in | mirror the image around a line: axis(0 x,1 y,2 both,3 diagonal,4 anti-diagonal,5 eight-fold,6 both diagonals) offset flip transfer(0 copy,1 add,2 sub,3 mul,4 add sub,5 max,6 min) |
| `skew` | in | shear that keeps tiling: skew_x skew_y (whole numbers) align(0 centre,1 top left,2 bottom right) |
| `auto_levels` | in | stretch values to the full range: per_channel |
| `channel_mixer` | in | mix colour channels: rr rg rb gr gg gb br bg bb mono |
| `alpha_merge` | rgb, alpha | put a grayscale image into the alpha channel |
| `color_to_mask` | in | mask of pixels close to a colour: r g b range softness |
| `replace_color` | in | replace one colour with another: src_r src_g src_b dst_r dst_g dst_b range softness |
| `pow` | in | raise values to a power: exponent |
| `histogram_shift` | in | shift all values: position |
| `grayscale_conversion` | in | weighted grayscale: r g b |
| `sobel` | in | sobel edge magnitude: intensity |
| `glow` | in | glow around bright areas: width intensity clear glow_r glow_g glow_b |
| `mosaic` | in, mosaic_map | mosaic of the map regions (or a pixel grid without a map): count intensity |
| `non_uniform_directional_warp` | in, intensity, angle | warp along a direction chosen per pixel: intensity angle |
| `multi_directional_warp` | in, warp | warp in several directions at once: intensity directions(0 one,1 two,2 four) angle mode(0 average,1 max,2 min,3 chain) |
| `rgba_split` | in | split into R, G, B and A grayscale outputs |
| `alpha_split` | in | split into an RGB output and an alpha output |
| `shape_stroke` | in | outline of a shape: width offset softness threshold opacity color_r color_g color_b |
| `shape_glow` | in | glow around a shape: width softness intensity threshold opacity spread color_r color_g color_b |
| `chrominance_extract` | in | colour without brightness |
| `shape_drop_shadow` | in | drop shadow of a mask: angle distance blur opacity spread |
| `curve` | in | tone curves: points (master), points_r points_g points_b points_a as "x:y,x:y" lists; a point may add a slope (x:y:s), two slopes (x:y:sin:sout) or Bezier handles at absolute positions (x:y:lx:ly:rx:ry, Substance Designer's left / right tangents) |
| `channel_shuffle` | in, in2 | rebuild channels: src_r src_g src_b src_a (0-3 channels of in, 4-7 channels of in2, 8 zero, 9 one), inv_r inv_g inv_b inv_a |
| `contrast_luminosity` | in | contrast around mid-gray and brightness shift: contrast luminosity |
| `srgb_to_linear` | in | decode the sRGB curve (colour channels only) |
| `linear_to_srgb` | in | encode the sRGB curve (colour channels only) |
| `premultiply` | in | alpha premultiplication: mode(0 straight to premultiplied,1 premultiplied to straight) |
| `histogram_equalize` | in | spread values evenly over 0..1: amount per_channel |
| `min_max` | a, b | per-channel minimum or maximum of two images: mode(0 min,1 max) |
| `morphology` | in | grow or shrink bright areas: mode(0 dilate,1 erode,2 open,3 close) radius |
| `multi_switch` | a, b, c, d, e, f, g, h | pass one of up to eight inputs: index |
| `color_lut` | in | colour grade through a lookup table file (.cube, .3dl, or a strip / Hald image): file="grade.cube" intensity |
| `lookup_table` | in, lut | recolour through a lookup image: mode(0 per channel,1 by luminance) row |
| `frequency_split` | in | split into a blurred low band and a high band centred on gray (outputs: low, high): radius |
| `frequency_merge` | low, high | recombine the bands of frequency_split: gain |
| `auto_crop` | in | stretch the non-black area to fill the image: threshold margin keep_aspect |
| `polar_convert` | in | mode(0 cartesian to polar,1 polar to cartesian) radius rotation tile |
| `clone_patch` | in | copy a patch from one place to another: src_x src_y dst_x dst_y width height feather rotation opacity |
| `noise_upscale` | in | smoothly enlarge a coarse image: cells mode(0 bicubic,1 bilinear,2 nearest) |
| `symmetry_slice` | in | kaleidoscope of one slice around the centre: slices mirror rotation tile |
| `quad_transform` | in | move the four corners: tl_x tl_y tr_x tr_y br_x br_y bl_x bl_y tile |
| `trapezoid_transform` | in | perspective taper: top bottom skew tile |
| `palette_extract` | in | strip of the dominant colours of an image: count |
| `palette_apply` | in, palette | snap colours to the nearest palette colour: count amount |
| `make_it_tile_mirror` | in | make an image tile by mirroring: mode(0 both axes,1 horizontal,2 vertical) |
| `halftone` | in | dots whose size follows brightness: cells mode(0 grid,1 diagonal) softness gain |
| `reaction_diffusion` | in | growing spots and stripes seeded by the input: resolution iterations feed kill spots seed |
| `fxaa` | in | smooth jagged edges: strength threshold |
| `bilateral_blur` | in, guide | blur that keeps edges: radius range |
| `path_warp` | in | push, slide or swirl a picture near a spline: width amount mode(0 push away,1 slide along,2 swirl) file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse |
| `shape_extrude` | in | extrude shapes along a direction (outputs: height, walls): length angle top falloff contrast invert |
| `shape_mapper` | in,pattern | put a pattern into every flood-filled region: scale rotation_random flip_random keep_aspect seed contrast invert |
| `cross_section` | in | plot of the values along one row: position height thickness fill contrast invert |
| `uber_emboss` | height,mask,gloss | emboss with gloss (outputs: emboss, highlight, shadow): intensity angle elevation gloss highlight gain |
| `diffusion` | in,mask | fill unknown pixels from the known ones (alpha or mask): smoothing gray |
| `summed_area_table` | in | running sums of the image: per_channel normalize |
| `vector_morph` | in,vector | move pixels along a vector map and fill the holes: amount directx |
| `safe_transform` | in | transform that stays seamless: mode(0 tile-safe,1 free) angle scale_x scale_y offset_x offset_y filter(0 nearest,1 bilinear,2 bicubic) edge(0 repeat,1 mirror,2 clamp,3 empty) rotate_after_scale |

## Material

| Node | Inputs | Description and parameters |
|---|---|---|
| `curvature_smooth` | in | smooth curvature from a normal map |
| `height_blend` | bg, fg, mask | blend two height maps: opacity contrast output |
| `edge_wear` | in, noise | edge wear mask from a height map: amount width contrast |
| `dirt` | in, noise | dirt mask from an ambient occlusion map: amount contrast |
| `dust` | in, noise | dust mask from a normal map: amount contrast |
| `mesh_weathering` | curvature, ao, normal, noise | wear, dirt and dust masks from baked maps: wear dirt dust up_angle(degrees) up_spread breakup scale contrast invert seed |
| `normal` | in | height->normal (OpenGL +Y): strength directx alpha_from_input |
| `normal_blend` | a, b, mask | blend normal maps (a = background, b = foreground): opacity |
| `ao` | in | ambient occlusion: mode(0 blur,1 horizon) radius intensity hbao_radius hbao_intensity quality(0 4,1 8,2 16 samples) use_world_units height_depth surface_size height_depth_cm |
| `curvature` | in | curvature of a normal map: mode(0 normal map,1 height map) intensity directx radius |
| `normal_invert` | in | flip normal map channels: invert_x invert_y invert_z invert_a |
| `normal_normalize` | in | renormalise a normal map |
| `normal_vector_rotation` | in, rotation | rotate normal vectors: angle directx |
| `normal_to_height` | in | integrate a normal map into a height map: directx relief_balance opacity |
| `normal_combine` | base, detail | combine two normal maps: technique(0 whiteout,1 channel mixer,2 detail oriented) |
| `bent_normal` | in | bent normal of a height map (outputs: normal, ao): radius relief directions steps directx |
| `normal_facing` | in | mask of surfaces that face a direction: angle tilt sharpness |
| `height_normal_blend` | normal, height | add the relief of a height map to a normal map: strength mode(0 reoriented,1 whiteout,2 partial derivative) opacity directx |
| `height_to_normal_world` | in | normal map from real-world sizes: size height directx |
| `normal_transform` | in | transform a normal map and turn its vectors with it: offset_x offset_y scale_x scale_y rotation rotate_vectors directx |
| `normal_sobel` | in | edges or curvature of a normal map: mode(0 edges,1 curvature) intensity directx |
| `metal_rough_to_spec_gloss` | base_color, metallic, roughness | convert a metal / roughness material (outputs: diffuse, specular, glossiness) |
| `spec_gloss_to_metal_rough` | diffuse, specular, glossiness | convert a specular / glossiness material (outputs: base_color, metallic, roughness) |
| `material_blend` | bg_base, bg_normal, bg_height, bg_rough, bg_metal, bg_ao, fg_base, fg_normal, fg_height, fg_rough, fg_metal, fg_ao, mask | blend two complete materials (outputs: base_color, normal, height, roughness, metallic, ao): opacity height_influence sharpness |
| `height_combine` | a, b, mask | combine two height maps: mode(0 max,1 min,2 add,3 difference) amount |
| `convex_concave` | in | ridge and cavity masks of a height map (outputs: convex, concave): radius gain |
| `leak_streaks` | in, noise | streaks that run on from a mask like drips: length angle intensity breakup |
| `settled_dust` | in, noise | dust that settles on flat upward surfaces and in cavities: amount slope cavity relief radius |
| `snow_filter` | height,mask | snow on upward-facing surfaces (outputs: height, mask): coverage slope softness breakup height_bias depth steepness scale seed |
| `season_filter` | in,mask | recolour foliage for a season (0 spring,1 summer,2 autumn,3 winter): season strength variation foliage_hue foliage_range scale seed |
| `water_filter` | height,base | water in the low parts (outputs: color, mask, height, roughness): level softness tint_r tint_g tint_b murkiness wet_edge ripples ripple_scale dry_roughness wet_roughness seed |

## Function-graph nodes

Used inside graphs with `meta kind "function"`. Links carry values (float, float2-4, integer, integer2-4, boolean, string). Slots `a`, `b`, `c` are operands. Running a function never produces NaN or infinity.

Substance Designer's 161 function presets (its `sbs://functions.sbs` library: `Functions/Math/clamp`, `Functions/colorValues/HSVtoRGB`, the easings, the HSL/HSV/HCL/HSI conversions, the matrix and polar transforms, and so on) are built in (`src/libfuncs.rs`), so a material that drives a parameter with one of them renders without Designer installed. Designer's own `functions.sbs` is never read: every library function is built in, and a call to a `sbs://` library function that is not among them gives 0 with a note. (Library *graphs* — the `x_*` nodes — are a separate mechanism and unaffected.)

| Node | Inputs | Description and parameters |
|---|---|---|
| `f_out` | in | result of the function |
| `f_pin` | a | re-route a value link |
| `f_float` |  | constant float: v |
| `f_int` |  | constant integer: v |
| `f_bool` |  | constant boolean: v (0 or 1) |
| `f_string` |  | constant string: v |
| `f_vec2` | x, y | float2 from parts (x y, linked parts win over the typed values) |
| `f_vec3` | x, y, z | float3 from parts |
| `f_vec4` | x, y, z, w | float4 from parts |
| `f_ivec2` | x, y | integer2 from parts |
| `f_ivec3` | x, y, z | integer3 from parts |
| `f_ivec4` | x, y, z, w | integer4 from parts |
| `f_swizzle` | a | pick or reorder components: mask (xyzw or rgba, e.g. "zyx") |
| `f_neg` | a | negate |
| `f_abs` | a | absolute value |
| `f_sqrt` | a | square root (0 for negative input) |
| `f_exp` | a | e to the power a |
| `f_log` | a | natural logarithm (0 for input <= 0) |
| `f_log2` | a | base-2 logarithm (0 for input <= 0) |
| `f_sin` | a | sine (radians) |
| `f_cos` | a | cosine (radians) |
| `f_tan` | a | tangent (radians) |
| `f_asin` | a | inverse sine (input clamped to -1..1) |
| `f_acos` | a | inverse cosine (input clamped to -1..1) |
| `f_atan` | a | inverse tangent |
| `f_radians` | a | degrees to radians |
| `f_degrees` | a | radians to degrees |
| `f_floor` | a | round down |
| `f_ceil` | a | round up |
| `f_round` | a | round to nearest |
| `f_fract` | a | fractional part |
| `f_sign` | a | -1, 0 or 1 |
| `f_saturate` | a | clamp to 0..1 |
| `f_not` | a | boolean not |
| `f_length` | a | length of a vector |
| `f_normalize` | a | vector of length 1 (zero stays zero) |
| `f_add` | a, b | a + b |
| `f_sub` | a, b | a - b |
| `f_mul` | a, b | a * b |
| `f_div` | a, b | a / b (0 when b is 0; integer division truncates) |
| `f_mod` | a, b | remainder of a / b with the sign of b (0 when b is 0) |
| `f_pow` | a, b | a to the power b |
| `f_min` | a, b | smaller value |
| `f_max` | a, b | larger value |
| `f_atan2` | a, b | angle of the point (b, a) |
| `f_step` | a, b | 0 when b < a, else 1 |
| `f_dot` | a, b | dot product |
| `f_cross` | a, b | cross product of two float3 |
| `f_distance` | a, b | distance between two points |
| `f_clamp` | a, b, c | clamp a between b (low) and c (high) |
| `f_lerp` | a, b, c | a + (b - a) * c |
| `f_smoothstep` | a, b, c | smooth 0..1 ramp of c between edges a and b |
| `f_eq` | a, b | a == b |
| `f_ne` | a, b | a != b |
| `f_lt` | a, b | a < b |
| `f_le` | a, b | a <= b |
| `f_gt` | a, b | a > b |
| `f_ge` | a, b | a >= b |
| `f_and` | a, b | boolean and |
| `f_or` | a, b | boolean or |
| `f_xor` | a, b | boolean exclusive or |
| `f_if` | cond, a, b | a when cond is true, else b (only the chosen branch runs) |
| `f_sequence` | a, b | run a, then b; returns b |
| `f_set` | a | store a in the variable `name`; returns a |
| `f_get` |  | read the variable `name` (0 until it is set) |
| `f_random` | a, b | random float between a (default 0) and b (default 1); `seed` shifts the sequence |
| `f_sample_gray` | a | brightness of input image `slot` (default in) at the position a (float2, 0..1) |
| `f_sample_color` | a | colour (float4) of input image `slot` at the position a |
| `f_image_avg` |  | average colour (float4) of input image `slot` |
| `f_image_min` |  | smallest channel values (float4) of input image `slot` |
| `f_image_max` |  | largest channel values (float4) of input image `slot` |
| `f_pos` |  | position being evaluated (float2, 0..1) |
| `f_size` |  | output size in pixels (float2) |
| `f_index` |  | instance / iteration number (integer) |
| `f_count` |  | number of instances / iterations (integer) |
| `f_seed` |  | random seed of the caller (integer) |
| `f_input` |  | input of this function: name, type (0 float, 1 int, 2 bool, 3 float2, 4 float3, 5 float4, 6 int2, 7 int3, 8 int4, 9 string), default v / x y z w |
| `f_graph_input` |  | value of an Input Value node of the graph the caller lives in: name, type, default v / x y z w |
| `f_to_float` | a | convert to float |
| `f_to_int` | a | convert to integer (drops the fraction) |
| `f_to_bool` | a | non-zero is true |
| `f_to_string` | a | text of a value |
| `f_concat` | a, b | join two strings |
| `f_call` | a, b, c, d | run another function (`fn`); a b c d fill its inputs in the order of their node numbers |

## Pattern-program nodes

Used inside graphs with `meta kind "fxmap"`, run by the `fx_map` node. A cell (centre, size, rotation, value, colour) is passed down; quadrants and iterations change it, markers draw.

| Node | Inputs | Description and parameters |
|---|---|---|
| `fxm_out` | in | start of a pattern program |
| `fxm_marker` |  | draw a shape: shape (0 circle 1 square 2 diamond 3 image) src x y w h rot value r g b softness rnd_pos rnd_scale rnd_rot rnd_val rnd_color |
| `fxm_quadrant` | q0, q1, q2, q3 | split the cell in four: scale off_x off_y p0..p3 (chance) v0..v3 (value) r0..r3 (rotation) rnd_pos rnd_scale rnd_rot rnd_val |
| `fxm_iterate` | body | repeat body: count dx dy dscale drot dval, or grid_x grid_y stagger; rnd_x rnd_y rnd_scale rnd_rot rnd_val |
| `fxm_switch` | a, b, c, d | run one branch: mode (0 index, 1 random) index w0..w3 |

## Splines and path nodes

Every path node can merge up to four splines (`file`, `file2`, `file3`, `file4`) and place them with `path_scale`, `path_rotate` (turns), `path_x`, `path_y`, `path_mirror` (0 none, 1 horizontal, 2 vertical, 3 both) and `path_reverse`; `path` and `path_count` choose a range of paths. `path_sample` writes position, width, height and side maps, `path_warp` bends a picture near a spline, and `path_splatter` repeats a pattern in rows along it. A mask or any picture can be turned into a spline with `materialcraft trace` or the node menu entry Trace to spline.

A spline is a list of paths made of points with optional bezier handles and a width factor. It is stored in the package as an embedded text resource (usually `name.spl`, see `UXMAP.md` section 3 for the format) and a path node names it with `file="name.spl"`. Without a file the node uses a built-in shape (`preset`: 0 line, 1 diagonal, 2 wave, 3 circle, 4 spiral, 5 zigzag, 6 figure eight, with `amplitude` and `frequency`). `path` picks one path of the spline (-1 = all). Coordinates are texture coordinates, so a spline wraps with the texture and the result always tiles. Widths and distances are given in pixels for a 512 px texture and scale with the size. With `fit=1` dashes, sleepers, stitches and scattered copies are spaced so the pattern meets itself where a path wraps around the texture. The editor draws and edits splines in the 2D view; `Import SVG paths` turns the outlines of a drawing into a spline.

## Mesh baking

The mesh map baker (Tools > Mesh map baker, or `materialcraft bake`) turns a low-poly mesh with UVs, optionally with high-poly detail and a cage, into texture maps. Meshes can be OBJ, glTF / GLB (including external `.bin` buffers and data URIs), PLY (ASCII and binary) or STL (ASCII and binary). Several low and several high files can be baked together.

Maps: tangent-space normal, world-space normal, position, ambient occlusion, bent normal (tangent and world space), curvature, thickness, height from cage, object ID, material ID, polygroup ID, vertex colour, opacity (ray hits), world-space direction and UV layout. Settings: size (up to 16384 px, non-square allowed), anti-aliasing 1x1 to 8x8, padding, front and rear ray distance, cage mesh or averaged-normal cage, match low and high objects by name suffix (`_low` / `_high` and others), normal handling per mesh (file normals, flat, smooth by angle, smoothing groups), per-vertex or per-face tangent basis, OpenGL or DirectX normal maps, first or second UV set, a single material, back-face culling, and the ray counts and distances for AO, thickness and curvature. The baker runs on a background thread with progress and Cancel and reports coverage, ray statistics and warnings.

Results are stored in the package as 16-bit PNG resources named `bake/<set>/<map>.png` (and the set's settings as `bake/<set>.cfg`), so they travel with the file and publish with it. A `baked_map` node reads one map by set and map name and resamples it to the graph size; `mesh_weathering` and the older `edge_wear`, `dirt`, `dust`, `leak_streaks` and `settled_dust` nodes turn baked curvature, AO and normal maps into masks.

## Library graphs implemented natively

Substance Designer library graphs that MaterialCraft runs with its own implementation of the whole graph (kind names start with `x_`; source in `src/nodes10*.rs`). An imported `.sbs` instance of one of these graphs becomes this node with the instance's parameter values; in the editor they appear in the node menu under the library graph's own name. Parameters keep the library's identifiers (vectors are split into `name_0`, `name_1`, ...), and every output of the library graph is kept.

Spline, path and splatter data travel between these nodes as images in MaterialCraft's own encoding, and 3D textures are slice atlases (`grid_size` slices in a grid, the first at the top left), so these nodes work with each other rather than with data images made by other programs.

### Filters

| Kind | Name | Library graph | Inputs | Outputs |
| --- | --- | --- | --- | --- |
| `x_3d_linear_gradient` | 3D Linear Gradient | `3d_gradient/3d_linear_gradient` | position | output |
| `x_3d_planar_projection` | 3D Planar Projection | `3d_planar_projection/3d_planar_projection` | position, wsnormal, texture | output |
| `x_3d_planar_projection_color` | 3D Planar Projection Color | `3d_planar_projection/3d_planar_projection_color` | position, wsnormal, texture | output |
| `x_3d_texture_offset_color` | 3D Texture Offset Color | `3d_texture_transform/3d_texture_offset_color` | input | output |
| `x_3d_texture_offset_grayscale` | 3D Texture Offset Grayscale | `3d_texture_transform/3d_texture_offset_grayscale` | input | output |
| `x_3d_texture_sdf` | 3D Texture Sdf | `3d_texture_jump_flood/3d_texture_sdf` | mask_input | output |
| `x_3d_texture_surface_render` | 3D Texture Surface Render | `3d_texture_render/3d_texture_surface_render` | 3d_distance_field, environment | output |
| `x_3d_texture_volume_render_light` | 3D Texture Volume Render Light | `3d_texture_render/3d_texture_volume_render_light` | 3d_signed_distance_field, density | output |
| `x_3d_volume_mask` | 3D Volume Mask | `3d_volume_mask/3d_volume_mask` | position | output |
| `x_acescg_to_linear_srgb` | Acescg To Linear Srgb | `color_space_conversion/acescg_to_linear_srgb` | input | output |
| `x_acescg_to_srgb` | Acescg To Srgb | `color_space_conversion/acescg_to_srgb` | input | output |
| `x_anisotropic_kuwahara_color` | Anisotropic Kuwahara Color | `anisotropic_kuwahara/anisotropic_kuwahara_color` | input, anisotropy_angle_map, slope_map, radius_map, direction_map | output, direction_map |
| `x_anisotropic_kuwahara_grayscale` | Anisotropic Kuwahara Grayscale | `anisotropic_kuwahara/anisotropic_kuwahara_grayscale` | input, anisotropy_angle_map, slope_map, radius_map, direction_map | output, direction_map |
| `x_ao_cancel` | AO Cancel | `ao_cancel/ao_cancel` | color, AO | output |
| `x_apply_color_palette` | Apply Color Palette | `quantize_color/apply_color_palette` | id, palette | output |
| `x_atlas_scatter` | Atlas Scatter | `atlas_scatter/atlas_scatter` | basecolor, normal, roughness, metallic, height, opacity, background_basecolor, background_normal, background_roughness, background_metallic, background_height, pattern_distribution_map, shape_scale_map, shape_rotation_map, height_offset_map, height_scale_map, mask_random_map, vector_map | basecolor, normal, roughness, metallic, height, shape_count, mask |
| `x_atlas_spliter` | Atlas Spliter | `atlas_spliter/atlas_spliter` | basecolor, normal, roughness, metallic, height, opacity, user1, user2 | basecolor, normal, roughness, metallic, height, opacity, shape_count, user1, user2 |
| `x_basecolor_metallic_roughness_converter` | Basecolor Metallic Roughness Converter | `pbr_converter/basecolor_metallic_roughness_converter` | basecolor, normal, roughness, metallic, specularlevel | diffuse, normal, specular, speuclar_face_color, reflection, glossiness, roughness, ior, f0 |
| `x_bitmap2material_2_light` | Bitmap2Material 2 Light | `bitmaptomaterial_2_light/Bitmap2Material_2_Light` | bitmap_diffuse, metallic_1 | diffuse, normal, specular, glossiness, roughness, metallic, ambient_occlusion, height |
| `x_clone` | Clone | `clone/clone` | Source, Mask | output |
| `x_color_equalizer` | Color Equalizer | `color_equalizer/color_equalizer` | input, mask_input | output, mask |
| `x_color_temperature_adjustment` | Color Temperature Adjustment | `color_temperature_adjustment/color_temperature_adjustment` | input | output |
| `x_create_color_palette_16colors` | Create Color Palette 16colors | `quantize_color/create_color_palette_16colors` | palette | palette, palette_color_amount |
| `x_crop` | Crop | `crop/crop` | input | output, area |
| `x_crop_grayscale` | Crop Grayscale | `crop/crop_grayscale` | input | output, area |
| `x_directional_distance` | Directional Distance | `directional_distance/directional_distance` | input, distance_map, angle_map | output, uv |
| `x_exposure` | Exposure | `exposure/exposure` | input | output |
| `x_extend_shape_color` | Extend Shape Color | `extend_shape/extend_shape_color` | input | output |
| `x_extend_shape_grayscale` | Extend Shape Grayscale | `extend_shape/extend_shape_grayscale` | input | output |
| `x_floodfill_mapper_color` | Floodfill Mapper Color | `floodfill_mapper/floodfill_mapper_color` | bbox, pattern_input_1, pattern_input_2, pattern_input_3, pattern_input_4, pattern_input_5, pattern_input_6, pattern_input_7, pattern_input_8, pattern_distribution_map, scale_map, rotation_map, luminance_offset_map | output |
| `x_floodfill_mapper_grayscale` | Floodfill Mapper Grayscale | `floodfill_mapper/floodfill_mapper_grayscale` | bbox, pattern_input, pattern_input_2, pattern_input_3, pattern_input_4, pattern_input_5, pattern_input_6, pattern_input_7, pattern_input_8, pattern_distribution_map, scale_map, rotation_map, luminance_offset_map | output |
| `x_get_size_color` | Get Size Color | `value_get_size/get_size_color` | input | output |
| `x_get_size_grayscale` | Get Size Grayscale | `value_get_size/get_size_grayscale` | input | output |
| `x_hald_clut` | Hald Clut | `hald_clut/hald_clut` | input, lut | output |
| `x_hdr_exposure_preview` | HDR Exposure Preview | `hdr_exposure_preview/hdr_exposure_preview` | input | output |
| `x_hdr_merge_2` | HDR Merge 2 | `hdr_merge/hdr_merge_2` | input_1, input_2, input_3, input_4, input_5, input_6, input_7, input_8, input_9, input_10, input_11, input_12, input_13, input_14, input_15, input_16 | output |
| `x_hdr_range_viewer` | HDR Range Viewer | `hdr_range_viewer/hdr_range_viewer` | input | output |
| `x_hdr_range_viewer_grayscale` | HDR Range Viewer Grayscale | `hdr_range_viewer/hdr_range_viewer_grayscale` | input | output |
| `x_height_extrude` | Height Extrude | `height_extrude/height_extrude` | height | depth, normal |
| `x_height_map_frequencies_mapper` | Height Map Frequencies Mapper | `height_map_frequencies_mapper/height_map_frequencies_mapper` | Height | Displacement, Relief_Parallax |
| `x_histogram_compute` | Histogram Compute | `histogram_tools/histogram_compute` | input | histogram, cdf |
| `x_histogram_render` | Histogram Render | `histogram_tools/histogram_render` | input | output |
| `x_id_to_mask_grayscale` | ID To Mask Grayscale | `id_to_mask/id_to_mask_grayscale` | id | output |
| `x_lighting_cancel_hf` | Lighting Cancel HF | `lighting_cancel_hf/lighting_cancel_hf` | Source | output |
| `x_lighting_cancel_lf` | Lighting Cancel LF | `lighting_cancel_lf/lighting_cancel_lf` | Source | output |
| `x_linear_srgb_to_acescg` | Linear Srgb To Acescg | `color_space_conversion/linear_srgb_to_acescg` | input | output |
| `x_median_filter_color` | Median Filter Color | `median_filter/median_filter_color` | input | output |
| `x_median_filter_grayscale` | Median Filter Grayscale | `median_filter/median_filter_grayscale` | input | output |
| `x_mlv_color` | MLV Color | `mean_of_least_variance/mlv_color` | input | output |
| `x_mlv_grayscale` | MLV Grayscale | `mean_of_least_variance/mlv_grayscale` | input | output |
| `x_modify_color_palette` | Modify Color Palette | `quantize_color/modify_color_palette` | id, palette | output, palette |
| `x_multi_clone_patch` | Multi Clone Patch | `clone_patch/multi_clone_patch` | input_1, input_2, input_3, input_4, input_5, input_6, input_7, input_8 | output_1, output_2, output_3, output_4, output_5, output_6, output_7, output_8, areas |
| `x_multi_clone_patch_grayscale` | Multi Clone Patch Grayscale | `clone_patch/multi_clone_patch_grayscale` | input_1, input_2, input_3, input_4, input_5, input_6, input_7, input_8 | output_1, output_2, output_3, output_4, output_5, output_6, output_7, output_8, areas |
| `x_multi_color_equalizer` | Multi Color Equalizer | `color_equalizer/multi_color_equalizer` | input_1, input_2, input_3, input_4, input_5, input_6, input_7, input_8, mask_input | output_1, output_2, output_3, output_4, output_5, output_6, output_7, output_8 |
| `x_multi_crop` | Multi Crop | `crop/multi_crop` | input_1, input_2, input_3, input_4, input_5, input_6, input_7, input_8 | output_1, output_2, output_3, output_4, output_5, output_6, output_7, output_8, area |
| `x_multi_crop_grayscale` | Multi Crop Grayscale | `crop/multi_crop_grayscale` | input_1, input_2, input_3, input_4, input_5, input_6, input_7, input_8 | output_1, output_2, output_3, output_4, output_5, output_6, output_7, output_8, area |
| `x_multiangle_to_albedo` | Multiangle To Albedo | `multiangle_to_albedo/multiangle_to_albedo` | input_1, input_2, input_3, input_4, input_5, input_6, input_7, input_8 | output |
| `x_multiangle_to_normal` | Multiangle To Normal | `multiangle_to_normal/multiangle_to_normal` | input_1, input_2, input_3, input_4, input_5, input_6, input_7, input_8 | output |
| `x_nadir_extract` | Nadir Extract | `panorama_nadir_extract/nadir_extract` | input | output |
| `x_non_square_transform` | Non Square Transform | `non_square_transform/non_square_transform` | input | output |
| `x_non_square_transform_greyscale` | Non Square Transform Greyscale | `non_square_transform/non_square_transform_greyscale` | input | output |
| `x_non_uniform_rotation_color` | Non Uniform Rotation Color | `non_uniform_rotation/non_uniform_rotation_color` | input, rotation_map, rotation_pivot_map | output |
| `x_non_uniform_rotation_grayscale` | Non Uniform Rotation Grayscale | `non_uniform_rotation/non_uniform_rotation_grayscale` | input, rotation_map, rotation_pivot_map | output |
| `x_normal_intensity` | Normal Intensity | `normal_intensity/normal_intensity` | input | output |
| `x_normal_uncombine` | Normal Uncombine | `normal_uncombine/normal_uncombine` | normal, height | normal, guessed_intensity |
| `x_rt_irradiance` | Rt Irradiance | `rt_irradiance/rt_irradiance` | basecolor, metallic, height, emissive, environment | output, raw_irradiance |
| `x_rt_shadows` | Rt Shadows | `rt_shadows_v2/rt_shadows` | input | output |
| `x_shadows` | Shadows | `shadows/shadows` | Input | output |
| `x_smart_auto_tile` | Smart Auto Tile | `smart_auto_tile/smart_auto_tile` | baseColor, normal, height, mask | baseColor, normal, height, pattern |
| `x_srgb_to_acescg` | Srgb To Acescg | `color_space_conversion/srgb_to_acescg` | input | output |
| `x_triplanar_3` | Triplanar 3 | `triplanar/triplanar_3` | position, ws_normal, input_x, input_y, input_z | output |
| `x_triplanar_grayscale_3` | Triplanar Grayscale 3 | `triplanar/triplanar_grayscale_3` | position, ws_normal, input_x, input_y, input_z | output |
| `x_uv_mapper_color` | UV Mapper Color | `uv_mapper/uv_mapper_color` | uv, input | output |
| `x_uv_mapper_grayscale` | UV Mapper Grayscale | `uv_mapper/uv_mapper_grayscale` | uv, input | output |
| `x_vertex_processor` | Vertex Processor | `paths_tools/vertex_processor` | paths, input_1, input_2, input_3, input_4, input_5, input_6, input_7, input_8 | output |
| `x_vertex_processor_simple` | Vertex Processor Simple | `paths_tools/vertex_processor_simple` | paths, input_1, input_2, input_3, input_4 | output |

### Generators

| Kind | Name | Library graph | Inputs | Outputs |
| --- | --- | --- | --- | --- |
| `x_2dtransform_path` | 2D Transform Path | `paths_tools/2dtransform_path` | paths | output |
| `x_3d_texture_position` | 3D Texture Position | `3d_texture_position/3d_texture_position` |  | output |
| `x_blackbody` | Blackbody | `blackbody/blackbody` |  | output |
| `x_cube_3d` | Cube 3D | `pattern_cube_3d/cube_3d` |  | output |
| `x_cube_3d_gbuffers` | Cube 3D Gbuffers | `pattern_cube_3d/cube_3d_gbuffers` |  | position, normal, depth, uv |
| `x_mask_to_paths_2` | Mask To Paths 2 | `paths_tools/mask_to_paths_2` | mask | preview, paths |
| `x_panorama_3d_position` | Panorama 3D Position | `panorama_position/panorama_3d_position` |  | output |
| `x_panorama_gradient_2_points` | Panorama Gradient 2 Points | `panorama_gradients/panorama_gradient_2_points` |  | output |
| `x_panorama_gradient_linear` | Panorama Gradient Linear | `panorama_gradients/panorama_gradient_linear` |  | output |
| `x_panorama_line_light` | Panorama Line Light | `panorama_shape/panorama_line_light` | background_image_input, shape_image_input, pattern_image_input | output |
| `x_panorama_nadir_patch` | Panorama Nadir Patch | `panorama_nadir_patch/panorama_nadir_patch` | input, mask_input | output |
| `x_panorama_physical_sun_sky` | Panorama Physical Sun Sky | `panorama_physical_sun_sky/panorama_physical_sun_sky` |  | output |
| `x_panorama_plane_light` | Panorama Plane Light | `panorama_shape/panorama_plane_light` | background_image_input, shape_image_input, pattern_image_input | output |
| `x_panorama_rotation` | Panorama Rotation | `panorama_rotation/panorama_rotation` | input | output |
| `x_panorama_shape` | Panorama Shape | `panorama_shape/panorama_shape` | input | output |
| `x_panorama_shape_2` | Panorama Shape 2 | `panorama_shape/panorama_shape_2` | background_image_input, shape_image_input | output |
| `x_panorama_sphere_light` | Panorama Sphere Light | `panorama_shape/panorama_sphere_light` | background_image_input, sphere_image_input | output |
| `x_panorama_straighten_horizon` | Panorama Straighten Horizon | `panorama_straighten_horizon/panorama_straighten_horizon` | input | output |
| `x_path_to_spline` | Path To Spline | `paths_tools/path_to_spline` | paths | spline_coords, spline_data, spline_amount |
| `x_paths_polygon` | Paths Polygon | `paths_tools/paths_polygon` |  | output |
| `x_paths_trace` | Paths Trace | `paths_tools/paths_trace` | background, paths | output |
| `x_paths_warp` | Paths Warp | `paths_tools/paths_warp` | paths, gradient | output |
| `x_point_list` | Point List | `spline_tools/point_list` | preview, point_list_input | preview, point_list, point_number |
| `x_quad_transform_path` | Quad Transform Path | `paths_tools/quad_transform_path` | paths | output |
| `x_scatter_on_spline_color` | Scatter On Spline Color | `spline_tools/scatter_on_spline_color` | background, spline_coords, spline_data, pattern_input_1, pattern_input_2, pattern_input_3, pattern_input_4, pattern_input_5, pattern_input_6, pattern_input_7, pattern_input_8, scale_map, color_map, mask_map | output |
| `x_scatter_on_spline_grayscale` | Scatter On Spline Grayscale | `spline_tools/scatter_on_spline_grayscale` | background, spline_coords, spline_data, pattern_input_1, pattern_input_2, pattern_input_3, pattern_input_4, pattern_input_5, pattern_input_6, pattern_input_7, pattern_input_8, scale_map, height_map, mask_map | output |
| `x_scatter_splines_on_splines` | Scatter Splines On Splines | `spline_tools/scatter_splines_on_splines` | preview, spline_coords, spline_data, custom_spline_coords, custom_spline_data, scale_map, rotation_map | preview, spline_coords, spline_data, spline_amount |
| `x_scratches_generator_normal` | Scratches Generator Normal | `pattern_scratches_generator/scratches_generator_normal` |  | normal, mask |
| `x_select_path` | Select Path | `paths_tools/select_path` | paths | output |
| `x_shape_splatter` | Shape Splatter | `shape_splatter/shape_splatter` | background_height, splatter_data_1, pattern_1, pattern_2, pattern_3, pattern_4, pattern_5, pattern_6, pattern_7, pattern_8, pattern_distribution_map, shape_scale_map, shape_rotation_map, height_offset_map, height_scale_map, mask_random_map, vector_map | height, splatter_data_1, splatter_data_2 |
| `x_shape_splatter_blend` | Shape Splatter Blend | `shape_splatter/shape_splatter_blend` | background_color, pattern_1, pattern_2, pattern_3, pattern_4, pattern_5, pattern_6, pattern_7, pattern_8, color_input, splatter_data_1, splatter_data_2 | output |
| `x_shape_splatter_blend_color` | Shape Splatter Blend Color | `shape_splatter/shape_splatter_blend_color` | background_color, pattern_1, pattern_2, pattern_3, pattern_4, pattern_5, pattern_6, pattern_7, pattern_8, color_input, splatter_data_1, splatter_data_2 | output |
| `x_shape_splatter_data_extract` | Shape Splatter Data Extract | `shape_splatter/shape_splatter_data_extract` | splatter_data_1, splatter_data_2 | position, uv_shape_space, uv_texture_space, bounding_box_size, shape_index, pattern_id, shape_angle |
| `x_shape_splatter_to_mask` | Shape Splatter To Mask | `shape_splatter/shape_splatter_to_mask` | splatter_data_1 | output |
| `x_spline_2dtransform` | Spline 2D Transform | `spline_tools/spline_2dtransform` | preview, spline_coords, spline_data | preview, spline_coords, spline_data, spline_amount |
| `x_spline_append` | Spline Append | `spline_tools/spline_append` | preview_1, spline_coords_1, spline_data_1, preview_2, spline_coords_2, spline_data_2 | preview, spline_coords, spline_data, spline_amount |
| `x_spline_bridge_2_splines` | Spline Bridge 2 Splines | `spline_tools/spline_bridge_2_splines` | preview_1, spline_coords_1, spline_data_1, preview_2, spline_coords_2, spline_data_2, start_tangent_length_curve, start_tangent_rotation_curve, end_tangent_length_curve, end_tangent_rotation_curve | preview, spline_coords, spline_data, spline_amount |
| `x_spline_bridge_list_v2` | Spline Bridge List V2 | `spline_tools/spline_bridge_list_v2` | preview, spline_coords, spline_data | preview, spline_coords, spline_data, spline_amount |
| `x_spline_bridge_mapper_color` | Spline Bridge Mapper Color | `spline_tools/spline_bridge_mapper_color` | background, spline_coords, spline_data, color_map | color, height, uv, mask |
| `x_spline_bridge_mapper_grayscale` | Spline Bridge Mapper Grayscale | `spline_tools/spline_bridge_mapper_grayscale` | background, spline_coords, spline_data, color_map | color, height, uv, mask |
| `x_spline_circle` | Spline Circle | `spline_tools/spline_circle` | preview, spline_coords, spline_data | preview, spline_coords, spline_data, spline_amount |
| `x_spline_cubic` | Spline Cubic | `spline_tools/spline_cubic` | preview, spline_coords, spline_data | preview, spline_coords, spline_data, spline_amount |
| `x_spline_fill` | Spline Fill | `spline_tools/spline_fill` | preview, spline_coords, spline_data | output |
| `x_spline_flow_mapper` | Spline Flow Mapper | `spline_tools/spline_flow_mapper` | spline_coords, spline_data, background, attenuation_profile_curve | output |
| `x_spline_mapper_color` | Spline Mapper Color | `spline_tools/spline_mapper_color` | background, spline_coords, spline_data, color_map, height_map, twist_curve | color, height, uv, id |
| `x_spline_mapper_grayscale` | Spline Mapper Grayscale | `spline_tools/spline_mapper_grayscale` | background, spline_coords, spline_data, color_map, height_map, twist_curve | color, height, uv, id |
| `x_spline_merge_list` | Spline Merge List | `spline_tools/spline_merge_list` | preview, spline_coords, spline_data | preview, spline_coords, spline_data, spline_amount |
| `x_spline_poly_quadratic` | Spline Poly Quadratic | `spline_tools/spline_poly_quadratic` | preview, spline_coords, spline_data, points_preview, point_list_input | preview, spline_coords, spline_data, spline_amount |
| `x_spline_quadratic` | Spline Quadratic | `spline_tools/spline_quadratic` | preview, spline_coords, spline_data | preview, spline_coords, spline_data, spline_amount |
| `x_spline_render` | Spline Render | `spline_tools/spline_render` | background, spline_coords, spline_data | output |
| `x_spline_sample_height` | Spline Sample Height | `spline_tools/spline_sample_height` | preview, spline_coords, spline_data, height_map | preview, spline_coords, spline_data, spline_amount |
| `x_spline_sample_thickness` | Spline Sample Thickness | `spline_tools/spline_sample_thickness` | preview, spline_coords, spline_data, thickness_map | preview, spline_coords, spline_data, spline_amount |
| `x_spline_select` | Spline Select | `spline_tools/spline_select` | preview, spline_coords, spline_data | preview, spline_coords, spline_data, spline_amount |
| `x_spline_warp` | Spline Warp | `spline_tools/spline_warp` | preview, spline_coords, spline_data, intensity_map, vector_map, attenuation_curve | preview, spline_coords, spline_data, spline_amount |

### Material

| Kind | Name | Library graph | Inputs | Outputs |
| --- | --- | --- | --- | --- |
| `x_bricks_001` | Bricks 001 | `bricks_001/bricks_001` |  | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_bricks_005` | Bricks 005 | `bricks_005/bricks_005` |  | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_cardboard_001` | Cardboard 001 | `cardboard_001/cardboard_001` |  | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_concrete_002` | Concrete 002 | `concrete_002/concrete_002` |  | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_concrete_006` | Concrete 006 | `concrete_006/concrete_006` |  | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_concrete_010` | Concrete 010 | `concrete_010/concrete_010` |  | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_concrete_011` | Concrete 011 | `concrete_011/concrete_011` |  | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_concrete_044` | Concrete 044 | `concrete_044/concrete_044` |  | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_concrete_070` | Concrete 070 | `concrete_070/concrete_070` |  | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_cracks_weathering` | Cracks Weathering | `cracks_weathering_material/cracks_weathering` | Input_9, Input_13, Input_6, Input, Input_8, Input_11, Input_12, Input_14, Input_2, Input_4, Input_10, Input_5 | Diffuse_1, basecolor, Normal, Specular, Glossiness, Roughness, Metallic, Height, ambient_occlusion |
| `x_fabric_002` | Fabric 002 | `fabric_002/fabric_002` |  | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_fabric_009` | Fabric 009 | `fabric_009/fabric_009` |  | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_fabric_025` | Fabric 025 | `fabric_025/fabric_025` |  | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_fabric_weathering` | Fabric Weathering | `fabric_weathering_material/fabric_weathering` | Diffuse_1, basecolor_1, Input, Specular_1, Input_1, Input_3, Input_4, Height_1, Input_2, AO, Normal_Wold_Space, Input_12 | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_gravel` | Gravel | `gravel/gravel` |  | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_leather_weathering` | Leather Weathering | `leather_weathering_material/leather_weathering` | Diffuse_1, basecolor_1, Input, Specular_1, Input_1, Input_3, Input_4, Height_1, Input_2, AO, Normal_Wold_Space, Input_12 | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_material_adjustment_blend` | Material Adjustment Blend | `material_adjustment_blend/material_adjustment_blend` | diffuse, basecolor, normal, specular, emissive, glossiness, roughness, metallic, specularlevel, height, ambientocclusion, opacity, Input_20, Input_21 | diffuse, basecolor, normal, specular, emissive, glossiness, roughness, metallic, specularlevel, height, ambientocclusion, opacity |
| `x_material_clone_patch` | Material Clone Patch | `clone_patch/material_clone_patch` | diffuse, basecolor, normal, specular, emissive, glossiness, roughness, metallic, specular_level, height, ambient_occlusion, opacity, anisotropy_level, anisotropy_angle, mask | diffuse, basecolor, normal, specular, emissive, glossiness, roughness, metallic, specular_level, height, ambient_occlusion, opacity, anisotropy_level, anisotropy_angle, mask, areas_basecolor, areas_normal, areas_height |
| `x_material_color_blend` | Material Color Blend | `material_color_blend/material_color_blend` | diffuse, basecolor, normal, specular, emissive, glossiness, roughness, metallic, specularlevel, height, ambientocclusion, opacity, Input_20, Input_21 | diffuse, basecolor, normal, specular, emissive, glossiness, roughness, metallic, specularlevel, height, ambientocclusion, opacity |
| `x_material_crop` | Material Crop | `crop/material_crop` | diffuse, basecolor, normal, specular, emissive, glossiness, roughness, metallic, specular_level, height, ambient_occlusion, opacity, anisotropy_level, anisotropy_angle, mask | diffuse, basecolor, normal, specular, emissive, glossiness, roughness, metallic, specular_level, height, ambient_occlusion, opacity, anisotropy_level, anisotropy_angle, mask, area |
| `x_material_height_blend` | Material Height Blend | `material_height_blend/material_height_blend` | material2_diffuse, material2_basecolor, material2_normal, material2_specular, material2_emissive, material2_glossiness, material2_roughness, material2_metallic, material2_specularlevel, material2_height, material2_ambientocclusion, material2_opacity, material1_diffuse, material1_basecolor, material1_normal, material1_specular, material1_emissive, material1_glossiness, material1_roughness, material1_metallic, material1_specularlevel, material1_height, material1_ambientocclusion, material1_opacity | diffuse, basecolor, normal, specular, emissive, glossiness, roughness, metallic, specularlevel, height, ambientocclusion, opacity, height_mask |
| `x_material_mesh_data_blender` | Material Mesh Data Blender | `material_mesh_data_blender/material_mesh_data_blender` | diffuse, basecolor, normal_2, specular, glossiness, roughness, metallic, Input_10, ambient_occlusion_2, ambient_occlusion, curvature, height, normal, vertex_color, normalWS | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_material_selector` | Material Selector | `material_selector/material_selector` | Input | output |
| `x_material_switch` | Material Switch | `material_switch/material_switch` | material2_diffuse, material2_basecolor, material2_normal, material2_specular, material2_emissive, material2_glossiness, material2_roughness, material2_metallic, material2_specularlevel, material2_height, material2_ambientocclusion, material2_opacity, material1_diffuse, material1_basecolor, material1_normal, material1_specular, material1_emissive, material1_glossiness, material1_roughness, material1_metallic, material1_specularlevel, material1_height, material1_ambientocclusion, material1_opacity | diffuse, basecolor, normal, specular, emissive, glossiness, roughness, metallic, specularlevel, height, ambientocclusion, opacity |
| `x_material_transform` | Material Transform | `material_transform/material_transform` | Input_12, Input_14, Input_11, Input_15, Input_20, Input_18, Input_13, Input_16, Input_21, Input_10, Input_19, Input_17 | diffuse, basecolor, normal, specular, emissive, glossiness, roughness, metallic, specularlevel, height, ambient_occlusion, opacity |
| `x_mesh_data_combiner` | Mesh Data Combiner | `mesh_data_combiner/mesh_data_combiner` | ambient_occlusion, curvature, height, position_grayscale, thickness, normal, position, uv_masks, vertex_color, direction, normalWS | ambient_occlusion, curvature, height, position_grayscale, thickness, normal, position, uv_masks, vertex_color, direction, normalWS |
| `x_metal_002` | Metal 002 | `metal_002/metal_002` |  | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_metal_003` | Metal 003 | `metal_003/metal_003` |  | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_metal_006` | Metal 006 | `metal_006/metal_006` |  | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_metal_plate_001` | Metal Plate 001 | `metal_plate_001/metal_plate_001` |  | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_metal_plate_004` | Metal Plate 004 | `metal_plate_004/metal_plate_004` |  | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_mg_bottom_to_top` | MG Bottom To Top | `mg_bottom_to_top/mg_bottom_to_top` | position, Input, mask | output |
| `x_mg_cloth_wear` | MG Cloth Wear | `mg_cloth_wear/mg_cloth_wear` | curvature, height, mask | output |
| `x_mg_dirt` | MG Dirt | `mg_dirt/mg_dirt` | ambient_occlusion, curvature, input, mask, position, world_space_normals | output |
| `x_mg_dripping_rust` | MG Dripping Rust | `mg_dripping_rust/mg_dripping_rust` | ambientOcclusion, curvature, position, mask_1 | output |
| `x_mg_dust` | MG Dust | `mg_dust/mg_dust` | ambient_occlusion, normalWS, noise, mask | output |
| `x_mg_edge_blur` | MG Edge Blur | `mg_edge_blur/mg_edge_blur` | curvature, mask | output |
| `x_mg_edge_damages` | MG Edge Damages | `mg_edge_damages/mg_edge_damages` | ambient_occlusion, curvature, mask | output |
| `x_mg_edge_dirt` | MG Edge Dirt | `mg_edge_dirt/mg_edge_dirt` | curvature, variation_mask, mask | output |
| `x_mg_edge_notch` | MG Edge Notch | `mg_edge_notch/mg_edge_notch` | curvature, mask | output |
| `x_mg_edge_select` | MG Edge Select | `mg_edge_select/mg_edge_select` | curvature, mask | output |
| `x_mg_edge_speckle` | MG Edge Speckle | `mg_edge_speckle/mg_edge_speckle` | curvature, variation_mask, mask | output |
| `x_mg_edge_wear` | MG Edge Wear | `mg_edge_wear/mg_edge_wear` | curvature, mask | output |
| `x_mg_fiber_glass_edge_wear` | MG Fiber Glass Edge Wear | `mg_fiber_glass_edge_wear/mg_fiber_glass_edge_wear` | ambient_occlusion, curvature, input, mask, position, normalWS | output |
| `x_mg_grease` | MG Grease | `mg_grease/mg_grease` | thickness, noise, mask | output |
| `x_mg_ground_dirt` | MG Ground Dirt | `mg_ground_dirt/mg_ground_dirt` | position, mask | output |
| `x_mg_leaks` | MG Leaks | `mg_leaks/mg_leaks` | ambient_occlusion, curvature, position, normalws, variation_mask, mask | output |
| `x_mg_leather_wear` | MG Leather Wear | `mg_leather_wear/mg_leather_wear` | ambient_occlusion, curvature, input, mask | output |
| `x_mg_light` | MG Light | `mg_light/mg_light` | normalWS | output |
| `x_mg_mask_builder_2` | MG Mask Builder 2 | `mg_mask_builder/mg_mask_builder_2` | ambient_occlusion, curvature, position, normalWS, grunge, grunge2, scatter, mask | output |
| `x_mg_metal_edge_wear` | MG Metal Edge Wear | `mg_metal_edge_wear/mg_metal_edge_wear` | ambient_occlusion, curvature, position, normalWS, input, mask | output |
| `x_mg_paint_wear` | MG Paint Wear | `mg_paint_wear/mg_paint_wear` | ambient_occlusion, curvature, variation_mask, mask | output |
| `x_mg_selective_dirt` | MG Selective Dirt | `mg_selective_dirt/mg_selective_dirt` | curvature, variation_mask, mask | output |
| `x_mg_sun_bleach` | MG Sun Bleach | `mg_sun_bleach/mg_sun_bleach` | ambient_occlusion, normalws, mask | output |
| `x_mg_surface_brush` | MG Surface Brush | `mg_surface_brush/mg_surface_brush` | ambient_occlusion, curvature, position, world_space_normal, mask | output |
| `x_moss_weathering` | Moss Weathering | `moss_weathering_material/moss_weathering` | Input_2, Input_5, Input, Input_7, Input_6, Input_8, Input_9, Input_11, Input_10, Input_3, Input_4, Input_12 | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, ambient_Oolusion, height |
| `x_multi_material_blend` | Multi Material Blend | `multi_material_blend/multi_material_blend` | material16_diffuse, material16_basecolor, Input_123, material16_specular, material16_emissive, material16_glossiness, material16_roughness, material16_metallic, material16_specularlevel, material16_height, material16_ambientocclusion, material16_opacity, material15_diffuse, material15_basecolor, Input_124, material15_specular, material15_emissive, material15_glossiness, material15_roughness, material15_metallic, material15_specularlevel, material15_height, material15_ambientocclusion, material15_opacity, material14_diffuse, material14_basecolor, Input_126, material14_specular, material14_emissive, material14_glossiness, material14_roughness, material14_metallic, material14_specularlevel, material14_height, material14_ambientocclusion, material14_opacity, material13_diffuse, material13_basecolor, material13_normal, material13_specular, material13_emissive, material13_glossiness, material13_roughness, material13_metallic, material13_specularlevel, material13_height, material13_ambientocclusion, material13_opacity, material12_diffuse, material12_basecolor, material12_normal, material12_specular, material12_emissive, material12_glossiness, material12_roughness, material12_metallic, material12_specularlevel, material12_height, material12_ambientocclusion, material12_opacity, material11_diffuse, material11_basecolor, material11_normal, material11_specular, material11_emissive, material11_glossiness, material11_roughness, material11_metallic, material11_specularlevel, material11_height, material11_ambientocclusion, material11_opacity, material10_diffuse, material10_basecolor, material10_normal, material10_specular, material10_emissive, material10_glossiness, material10_roughness, material10_metallic, material10_specularlevel, material10_height, material10_ambientocclusion, material10_opacity, material9_diffuse, material9_basecolor, material9_normal, material9_specular, material9_emissive, material9_glossiness, material9_roughness, material9_metallic, material9_specularlevel, material9_height, material9_ambientocclusion, material9_opacity, material8_diffuse, material8_basecolor, material8_normal, material8_specular, material8_emissive, material8_glossiness, material8_roughness, material8_metallic, material8_specularlevel, material8_height, material8_ambientocclusion, material8_opacity, material7_diffuse, material7_basecolor, material7_normal, material7_specular, material7_emissive, material7_glossiness, material7_roughness, material7_metallic, material7_specularlevel, material7_height, material7_ambientocclusion, material7_opacity, material6_diffuse, material6_basecolor, material6_normal, material6_specular, material6_emissive, material6_glossiness, material6_roughness, material6_metallic, material6_specularlevel, material6_height, material6_ambientocclusion, material6_opacity, material5_diffuse, material5_basecolor, material5_normal, material5_specular, material5_emissive, material5_glossiness, material5_roughness, material5_metallic, material5_specularlevel, material5_height, material5_ambientocclusion, material5_opacity, material4_diffuse, material4_basecolor, material4_normal, material4_specular, material4_emissive, material4_glossiness, material4_roughness, material4_metallic, material4_specularlevel, material4_height, material4_ambientocclusion, material4_opacity, material3_diffuse, material3_basecolor, material3_normal, material3_specular, material3_emissive, material3_glossiness, material3_roughness, material3_metallic, material3_specularlevel, material3_height, material3_ambientocclusion, material3_opacity, material2_diffuse, material2_basecolor, material2_normal, material2_specular, material2_emissive, material2_glossiness, material2_roughness, material2_metallic, material2_specularlevel, material2_height, material2_ambientocclusion, material2_opacity, material1_diffuse, material1_basecolor, material1_normal, material1_specular, material1_emissive, material1_glossiness, material1_roughness, material1_metallic, material1_specularlevel, material1_height, material1_ambientocclusion, material1_opacity, Input_160 | Diffuse, basecolor, Normal, Specular, Emissive, Glossiness, Roughness, Metallic, specularlevel, AmbientOcclusion, Height, opacity |
| `x_old_painted_planks` | Old Painted Planks | `old_painted_planks/old_painted_planks` |  | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_painted_metal_weathering` | Painted Metal Weathering | `painted_metal_weathering_material/painted_metal_weathering` | Diffuse, basecolor_1, Normal_High_Poly_1, Specular, Input, Input_1, Input_2, Height, Input_3, Normal_Wold_Space, AO, Input_12 | diffuse, basecolor, normal, Specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_paper_005` | Paper 005 | `paper_005/paper_005` |  | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_pbr_albedo_safe_color` | PBR Albedo Safe Color | `pbr_albedo_safe_color/pbr_albedo_safe_color` | basecolor_diffuse, metallic | output |
| `x_pbr_base_material` | PBR Base Material | `pbr_base_material/pbr_base_material` | diffuse_input, basecolor_input, normal_input, specular_input, glossiness_input, roughness_input, metallic_input, height_input, ambientOcclusion_input, grunge_input | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambientOcclusion |
| `x_pbr_basecolor_metallic_validate_2` | PBR Basecolor Metallic Validate 2 | `pbr_basecolor_metallic_validate/pbr_basecolor_metallic_validate_2` | baseColor, metallic | output |
| `x_pbr_dielectric_f0` | PBR Dielectric F0 | `pbr_dielectric_F0/pbr_dielectric_F0` |  | output |
| `x_pbr_metal_reflectance` | PBR Metal Reflectance | `pbr_metal_reflectance/pbr_metal_reflectance` |  | output |
| `x_pbr_render` | PBR Render | `pbr_render2/PBR_render` | basecolor, normal, emissive, roughness, metallic, specularlevel, height, ambient_occlusion, opacity, anisotropyLevel, anisotropyAngle, translucency, scatteringdistancescale, lens_dirt_map, lens_aperture_map, background_input, environment_map | output, raw_irradiance, raw_specular, normalws, normal, uv |
| `x_pbr_render_mapping_color` | PBR Render Mapping Color | `pbr_render2/PBR_render_mapping_color` | texture, uv | output |
| `x_pbr_render_mapping_grayscale` | PBR Render Mapping Grayscale | `pbr_render2/PBR_render_mapping_grayscale` | texture, uv | output |
| `x_rock_weathering` | Rock Weathering | `rock_weathering_material/rock_weathering` | Diffuse, basecolor_1, Input, Specular, Input_1, Input_3, Input_4, Height, Input_2, AO, Curvature, Normal_Wold_Space, Input_12 | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_rough_concrete_with_lines` | Rough Concrete With Lines | `rough_concrete_with_lines/rough_concrete_with_lines` |  | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_rust_weathering` | Rust Weathering | `rust_weathering_material/rust_weathering` | diffuse_1, basecolor_1, normal_1, specular_1, glossiness_1, roughness_1, metallic_1, height_1, ambientOcclusion_1, ambientOcclusion, curvature, position, mask_1 | Diffuse, basecolor, Normal, Specular, Glossiness, Roughness, Metallic, Height, ambient_occlusion |
| `x_snow_cover` | Snow Cover | `me_snow_cover/snow_cover` | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion, mask | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_tiles_002` | Tiles 002 | `tiles_002/tiles_002` |  | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_tiles_008` | Tiles 008 | `tiles_008/tiles_008` |  | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_tiles_011` | Tiles 011 | `tiles_011/tiles_011` |  | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_tiles_025` | Tiles 025 | `tiles_025/tiles_025` |  | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |
| `x_water_level` | Water Level | `me_water_level/water_level` | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion, mask | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion, mask |
| `x_wood_american_cherry` | Wood American Cherry | `wood_american_cherry/wood_american_cherry` |  | diffuse, basecolor, normal, specular, glossiness, roughness, metallic, height, ambient_occlusion |

## Planned node families

Described by what they do; see `RELEASE_PLAN.md` for the order.

- More noises: simplex and ridged 3D noises, and more grunge and dirt recipes.
- More patterns: brick variants.
- More adjustments: contrast / luminosity, linear / sRGB conversion, colour lookup tables, histogram select and equalise, frequency tools, pre-multiplied conversion.
- More effects: UV-aware diffusion variants.
- More transforms: auto crop, polar conversion, noise upscaling, safe transforms, trapezoid transforms.
- Normal tools: bent and facing normals, height-normal blending, world-unit height to normal, normal transform.
- Material tools: layers.
