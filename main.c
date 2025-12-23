// main.c logic loop
while (aptMainLoop()) {
    // ... input handling ...

    // Advance Flash Frame (usually 1000.0 / frame_rate)
    bridge_tick(player, 1000.0 / 30.0); 

    // Render?
    // In Phase 1, nothing will show up because we used NullRenderer.
    // In Phase 2, you will add: bridge_render(player);
}