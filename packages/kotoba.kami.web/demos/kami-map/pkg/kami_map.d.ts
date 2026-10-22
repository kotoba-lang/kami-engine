/* tslint:disable */
/* eslint-disable */

export class KamiMap {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Clear all overlay layers.
     */
    clear_layers(): void;
    /**
     * Initialize the map on a canvas element.
     */
    static create(canvas_id: string, options_json: string): Promise<KamiMap>;
    fly_to(lng: number, lat: number, zoom: number, duration_ms: number): void;
    /**
     * Render one frame. Call from requestAnimationFrame.
     */
    frame(dt_ms: number): void;
    get_viewport(): string;
    get_zoom(): number;
    on_pointer_down(x: number, y: number, button: number): void;
    on_pointer_move(_x: number, _y: number, dx: number, dy: number): void;
    on_pointer_up(_x: number, _y: number): void;
    on_wheel(delta: number): void;
    resize(width: number, height: number): void;
    set_bearing(degrees: number): void;
    set_center(lng: number, lat: number): void;
    set_pitch(degrees: number): void;
    /**
     * Add a GeoJSON route (line) layer.
     */
    set_route(coords_json: string, color_hex: string, width: number): void;
    set_zoom(zoom: number): void;
    /**
     * Get tile URLs that need fetching for the current viewport.
     * Returns JSON array of {z, x, y, url} objects.
     */
    tiles_to_fetch(): string;
    unproject(screen_x: number, screen_y: number): string;
    /**
     * Upload a tile image (RGBA bytes) and register it for rendering.
     */
    upload_tile(z: number, x: number, y: number, rgba_data: Uint8Array, img_width: number, img_height: number): void;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_kamimap_free: (a: number, b: number) => void;
    readonly kamimap_clear_layers: (a: number) => void;
    readonly kamimap_create: (a: number, b: number, c: number, d: number) => any;
    readonly kamimap_fly_to: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly kamimap_frame: (a: number, b: number) => void;
    readonly kamimap_get_viewport: (a: number) => [number, number];
    readonly kamimap_get_zoom: (a: number) => number;
    readonly kamimap_on_pointer_down: (a: number, b: number, c: number, d: number) => void;
    readonly kamimap_on_pointer_move: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly kamimap_on_pointer_up: (a: number, b: number, c: number) => void;
    readonly kamimap_on_wheel: (a: number, b: number) => void;
    readonly kamimap_resize: (a: number, b: number, c: number) => void;
    readonly kamimap_set_bearing: (a: number, b: number) => void;
    readonly kamimap_set_center: (a: number, b: number, c: number) => void;
    readonly kamimap_set_pitch: (a: number, b: number) => void;
    readonly kamimap_set_route: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly kamimap_set_zoom: (a: number, b: number) => void;
    readonly kamimap_tiles_to_fetch: (a: number) => [number, number];
    readonly kamimap_unproject: (a: number, b: number, c: number) => [number, number];
    readonly kamimap_upload_tile: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => void;
    readonly wasm_bindgen__closure__destroy__h1fc64ebb5598a658: (a: number, b: number) => void;
    readonly wasm_bindgen__convert__closures_____invoke__he5b12a5e6eb39525: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen__convert__closures_____invoke__h383bd871f37058d8: (a: number, b: number, c: any, d: any) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
