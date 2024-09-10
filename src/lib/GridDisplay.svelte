<script>
    import Grid from "svelte-grid";
    import gridHelp from "svelte-grid/build/helper/index.mjs";
    
    const id = () => "_" + Math.random().toString(36).substr(2, 9);
    
    let items = [
      {
        6: gridHelp.item({
          x: 0,
          y: 0,
          w: 2,
          h: 2,
        }),
        id: id(),
      },
    
      {
        6: gridHelp.item({
          x: 2,
          y: 0,
          w: 2,
          h: 2,
        }),
        id: id(),
      },
    ]
    
    const breakpoint = 1200
    const column = 6
    
    const cols = [[breakpoint, column]]
    </script>
    
    <Grid {cols} bind:items={items} let:item={item}>
      <div>{item.id}</div>
    </Grid>