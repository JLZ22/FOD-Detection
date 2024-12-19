<script lang='ts'>
	import CameraDisplay from "$lib/CameraDisplay.svelte";
	import ControlPanel from "$lib/ControlPanel.svelte";
	import { onMount } from "svelte";
	import {top, front, left} from "$lib/win_store.js";
	import { invoke } from "@tauri-apps/api/tauri";
	import { emit } from "@tauri-apps/api/event";
	
	onMount(() => {
		invoke('start_streaming');
		
		const unsubscribe_top = top.subscribe(value => {
			emit('update-camera-0', value);
		});
		const unsubscribe_front = front.subscribe(value => {
			emit('update-camera-1', value);
		});
		const unsubscribe_left = left.subscribe(value => {
			emit('update-camera-2', value);
		});

		return () => {
			unsubscribe_top();
			unsubscribe_front();
			unsubscribe_left();
		};
	});
</script>

<div class="grid-container">
    <div class="control-panel">
        <ControlPanel />
    </div>
    <div class="camera-display top">
        <CameraDisplay cameraId={$top} windowName="Top View" winId={0} />
    </div>
    <div class="camera-display front">
        <CameraDisplay cameraId={$front} windowName="Front View" winId={1} />
    </div>
    <div class="camera-display left">
        <CameraDisplay cameraId={$left} windowName="Left View" winId={2} />
    </div>
</div>

<style>
.grid-container {
    display: grid;
    grid-template-rows: 1fr 1fr;
    grid-template-columns: 1fr 1fr;
    height: 100vh; /* Adjust to fit the viewport */
}

.control-panel {
    grid-row: 1 / 2; /* Top-left */
    grid-column: 1 / 2;
    background-color: #f0f0f0; /* Optional: style for visual distinction */
}

.camera-display.top {
    grid-row: 1 / 2; /* Top-right */
    grid-column: 2 / 3;
    background-color: #d0d0f0; /* Optional */
}

.camera-display.front {
    grid-row: 2 / 3; /* Bottom-right */
    grid-column: 2 / 3;
    background-color: #f0d0d0; /* Optional */
}

.camera-display.left {
    grid-row: 2 / 3; /* Bottom-left */
    grid-column: 1 / 2;
    background-color: #d0f0d0; /* Optional */
}
</style>
