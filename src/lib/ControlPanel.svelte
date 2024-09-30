<script lang='ts'>
    import {win1} from '$lib/win1.js';
    import {win2} from '$lib/win2.js';
    import {win3} from '$lib/win3.js';
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
    <label for="win1">Win1:</label>
    {#if available_cameras.length > 0}
        <select id="win1" bind:value={$win1}>
            {#each available_cameras as option}
                <option value={option}>{option}</option>
            {/each}
        </select>
    {:else}
        <p>No cameras available</p>
    {/if}
</div>

<div>
    <label for="win2">Win2:</label>
    {#if available_cameras.length > 0}
        <select id="win2" bind:value={$win2}>
            {#each available_cameras as option}
                <option value={option}>{option}</option>
            {/each}
        </select>
    {:else}
        <p>No cameras available</p>
    {/if}
</div>

<div>
    <label for="win3">Win3:</label>
    {#if available_cameras.length > 0}
        <select id="win3" bind:value={$win3}>
            {#each available_cameras as option}
                <option value={option}>{option}</option>
            {/each}
        </select>
    {:else}
        <p>No cameras available</p>
    {/if}
</div>
