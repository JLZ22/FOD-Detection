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
			emit('update-camera-top', value);
		});
		const unsubscribe_front = front.subscribe(value => {
			emit('update-camera-front', value);
		});
		const unsubscribe_left = left.subscribe(value => {
			emit('update-camera-left', value);
		});


		return () => {
			unsubscribe_top();
			unsubscribe_front();
			unsubscribe_left();
		};
	});
</script>

<div>
	<ControlPanel/>
	<CameraDisplay cameraId={$top} windowName='Top View' winId={0}/>
	<CameraDisplay cameraId={$front} windowName='Front View' winId={1}/>
	<CameraDisplay cameraId={$left} windowName='Left View' winId={2}/>
</div>

