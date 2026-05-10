import uPlot from 'uplot';

/**
 * Night shading fill color — semi-transparent dark overlay for regions
 * where sun altitude is below the horizon.
 */
const NIGHT_FILL = 'rgba(0, 0, 30, 0.06)';

/**
 * Creates a uPlot draw hook that shades night regions on the chart.
 *
 * Night is defined as periods where sun_altitude < 0 (sun below horizon).
 * The hook draws semi-transparent rectangles over the full chart height
 * for contiguous night regions.
 *
 * @param sunAltitude - Array of sun altitude values (degrees) aligned with times
 * @param times - Array of unix timestamps (seconds) aligned with sunAltitude
 * @returns A uPlot draw hook function
 */
export function createDayNightHook(
    sunAltitude: number[],
    times: number[],
): (u: uPlot) => void {
    return (u: uPlot) => {
        const ctx = u.ctx;
        ctx.save();
        ctx.fillStyle = NIGHT_FILL;

        // Walk through the time series and find contiguous night regions
        let nightStart: number | null = null;

        for (let i = 0; i < times.length; i++) {
            const isNight = sunAltitude[i] < 0;

            if (isNight && nightStart === null) {
                // Start of a night region
                nightStart = times[i];
            } else if (!isNight && nightStart !== null) {
                // End of a night region — draw the rectangle
                drawNightRegion(u, ctx, nightStart, times[i]);
                nightStart = null;
            }
        }

        // If the series ends during night, close the final region
        if (nightStart !== null && times.length > 0) {
            drawNightRegion(u, ctx, nightStart, times[times.length - 1]);
        }

        ctx.restore();
    };
}

/**
 * Draws a single night region rectangle on the chart canvas.
 */
function drawNightRegion(
    u: uPlot,
    ctx: CanvasRenderingContext2D,
    startTime: number,
    endTime: number,
): void {
    const x0 = u.valToPos(startTime, 'x', true);
    const x1 = u.valToPos(endTime, 'x', true);

    // Clip to the plot area
    const left = Math.max(x0, u.bbox.left);
    const right = Math.min(x1, u.bbox.left + u.bbox.width);

    if (right > left) {
        ctx.fillRect(left, u.bbox.top, right - left, u.bbox.height);
    }
}

/**
 * Tooltip element ID used by the crosshair tooltip hook.
 */
const TOOLTIP_ID = 'uplot-crosshair-tooltip';

/**
 * Creates a uPlot cursor hook that displays a tooltip with values
 * at the current crosshair position.
 *
 * The tooltip shows the time and all visible series values at the
 * cursor's x-position. It is positioned near the cursor and hidden
 * when the cursor leaves the chart.
 *
 * @returns A uPlot setCursor hook function
 */
export function createCrosshairTooltipHook(): (u: uPlot) => void {
    return (u: uPlot) => {
        const { left, idx } = u.cursor;

        // Get or create the tooltip element
        let tooltip = u.root.querySelector(`#${TOOLTIP_ID}`) as HTMLDivElement | null;

        if (left === undefined || left < 0 || idx === undefined || idx === null) {
            // Cursor is outside the chart — hide tooltip
            if (tooltip) {
                tooltip.style.display = 'none';
            }
            return;
        }

        if (!tooltip) {
            tooltip = document.createElement('div');
            tooltip.id = TOOLTIP_ID;
            tooltip.style.position = 'absolute';
            tooltip.style.pointerEvents = 'none';
            tooltip.style.background = 'rgba(255, 255, 255, 0.95)';
            tooltip.style.border = '1px solid #ddd';
            tooltip.style.borderRadius = '4px';
            tooltip.style.padding = '6px 10px';
            tooltip.style.fontSize = '12px';
            tooltip.style.lineHeight = '1.4';
            tooltip.style.boxShadow = '0 2px 4px rgba(0,0,0,0.1)';
            tooltip.style.zIndex = '100';
            tooltip.style.whiteSpace = 'nowrap';
            u.root.appendChild(tooltip);
        }

        // Build tooltip content
        const lines: string[] = [];

        // Time label
        const time = u.data[0][idx];
        if (time !== undefined && time !== null) {
            const date = new Date(time * 1000);
            lines.push(`<strong>${date.toLocaleString(undefined, {
                month: 'short',
                day: 'numeric',
                hour: '2-digit',
                minute: '2-digit',
            })}</strong>`);
        }

        // Series values (skip series 0 which is the time axis)
        for (let i = 1; i < u.series.length; i++) {
            const series = u.series[i];
            if (!series.show) continue;

            // Skip hidden band-boundary series (no stroke or transparent stroke)
            const stroke = series.stroke;
            if (!stroke || stroke === 'transparent') continue;

            const value = u.data[i]?.[idx];
            if (value === null || value === undefined) continue;

            const label = series.label ?? `Series ${i}`;
            lines.push(`<span style="color:${typeof stroke === 'function' ? '#333' : stroke}">${label}: ${value.toFixed(1)}</span>`);
        }

        if (lines.length === 0) {
            tooltip.style.display = 'none';
            return;
        }

        tooltip.innerHTML = lines.join('<br>');
        tooltip.style.display = 'block';

        // Position tooltip near cursor, offset to the right
        const tooltipWidth = tooltip.offsetWidth;
        const plotRight = u.bbox.left + u.bbox.width;
        const cursorX = u.bbox.left + left;

        // Flip to left side if too close to right edge
        if (cursorX + tooltipWidth + 16 > plotRight) {
            tooltip.style.left = `${left - tooltipWidth - 10}px`;
        } else {
            tooltip.style.left = `${left + 16}px`;
        }

        tooltip.style.top = `${u.bbox.top / devicePixelRatio + 10}px`;
    };
}
