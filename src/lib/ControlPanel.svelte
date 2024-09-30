<script lang='ts'>
    import {top, front, left} from '$lib/win_store.js';
	import { onDestroy, onMount } from 'svelte';
    import { listen } from '@tauri-apps/api/event';
    import { invoke } from '@tauri-apps/api/tauri';

    let unlisten: (() => void) | undefined;
    let available_cameras: number[] = [];

    onMount (async () => {
        invoke('poll_and_emit_image_sources');

        try {
            unlisten = await listen('available-cameras', (event) => {
                available_cameras = event.payload as number[];
                console.log(available_cameras);
            });
        } catch (e) {
            available_cameras = [];
            unlisten = undefined;
        }
    });

    onDestroy(() => {
        if (unlisten) {
            unlisten();
        }
    });
</script>

<div>
    <label for="Top View">Top View:</label>
    {#if available_cameras.length > 0}
        <select id="top" bind:value={$top}>
            {#each available_cameras as option}
                <option value={option}>{option}</option>
            {/each}
        </select>
    {:else}
        <p>No cameras available</p>
    {/if}
</div>

<div>
    <label for="Right View">Right View:</label>
    {#if available_cameras.length > 0}
        <select id="front" bind:value={$front}>
            {#each available_cameras as option}
                <option value={option}>{option}</option>
            {/each}
        </select>
    {:else}
        <p>No cameras available</p>
    {/if}
</div>

<div>
    <label for="Left View">Left View:</label>
    {#if available_cameras.length > 0}
        <select id="left" bind:value={$left}>
            {#each available_cameras as option}
                <option value={option}>{option}</option>
            {/each}
        </select>
    {:else}
        <p>No cameras available</p>
    {/if}
</div>
