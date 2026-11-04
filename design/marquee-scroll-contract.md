# SIM-979 interaction contract

OpenPencil v22 retains the published v20's 143 pages and adds six editable states: shallow fine control, continuous deep-edge ramp, large logical range, return-to-content stop, reversed fine control and pending result in dark mode. No persistent speed control is added to the product.

The JSON next to this document is the runtime source of the calibration. Distances are logical pixels and speeds are viewport heights per second. Edge depth controls precision. Only continuous deep-edge residence unlocks the large-range ramp; a hysteresis band avoids accidental phase resets from small pointer noise. Returning to the content or reversing direction clears the accumulated duration. There is no coast after release. Mouse movement frequency never advances the clock.

The provisional large-range ceiling traverses the complete logical extent in roughly 18 seconds at full speed, after a gradual ramp. This is a tuning target, not a claim that every directory or display has passed a fixed traversal-time test. Near either end the controller brakes and clamps. After a long frame gap it restarts in the fine phase and caps the integration step, rather than catching up with a large jump.

Scroll by global f64 offset through the existing bounded 4096-row rebasing window. Keep the initial selection anchor in global coordinates. Visible cells use the current GridBand immediately; immutable masks are calculated on a latest-only worker. File operations must wait until the released selection has an accepted final result. A raw selection count or a visual highlight alone is not proof that a complete file command can use that selection.

Microsoft's [time-integration discussion](https://devblogs.microsoft.com/oldnewthing/20210126-00/?p=104759) explains why mouse-motion-driven increments can accidentally speed up scrolling. Its [distance-based variant](https://devblogs.microsoft.com/oldnewthing/20210127-00/?p=104764) also documents the screen-edge limitations of using only distance outside the window. The combined depth/dwell strategy here is a product choice, not an asserted Windows standard.

Native synthetic-controller measurements and actual OS mouse tests must be identified separately. Test at small, 100k and 10M item counts, narrow and regular windows, both directions, partial columns, additive selection, manual wheel input, release, cancellation, lock and activation loss. Check final selection identity, not just count. A long hold must cross several physical windows with bounded visible caches and pending requests.
