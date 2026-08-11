/**
 * Global search query store — TopBar writes, screens react.
 */

import { writable } from "svelte/store";

export const searchQuery = writable<string>("");
