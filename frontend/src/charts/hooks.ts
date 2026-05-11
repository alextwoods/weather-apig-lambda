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
 * CSS class used to locate the crosshair tooltip within each chart's root element.
 */
const TOOLTIP_CLASS = 'chart-crosshair-tooltip';

/**
 * Attribute set on u.root to track whether the mouse is directly over this chart.
 * Used to prevent synced charts from showing tooltips.
 */
const HOVER_ATTR = 'data-chart-hovered';

/**
 * Describes a visible series entry for tooltip rendering.
 * For fan charts, includes p10/p90 range values alongside the median.
 */
interface TooltipEntry {
    label: string;
    color: string;
    value: number;
    p10?: number | null;
    p90?: number | null;
}

/**
 * Creates a uPlot setCursor hook that displays a tooltip with values
 * at the current crosshair position.
 *
 * The tooltip shows the time and all visible series values at the
 * cursor's x-position. For fan chart series, it shows the median value
 * with the p10–p90 range in parentheses (e.g., "66 (62–67)").
 *
 * Only shows on the chart being directly hovered (not on synced charts).
 * Uses mouseenter/mouseleave tracking on u.over (the plot overlay element)
 * to determine hover state.
 *
 * @returns A uPlot setCursor hook function
 */
export function createCrosshairTooltipHook(): (u: uPlot) => void {
    let hoverListenersAttached = false;

    return (u: uPlot) => {
        // Attach hover tracking listeners once on the over element (the interactive plot area)
        if (!hoverListenersAttached) {
            hoverListenersAttached = true;
            // Ensure u.root can serve as positioning context
            const rootStyle = getComputedStyle(u.root);
            if (rootStyle.position === 'static') {
                u.root.style.position = 'relative';
            }

            u.over.addEventListener('mouseenter', () => {
                u.root.setAttribute(HOVER_ATTR, '1');
            });
            u.over.addEventListener('mouseleave', () => {
                u.root.removeAttribute(HOVER_ATTR);
                const tt = u.root.querySelector(`.${TOOLTIP_CLASS}`) as HTMLDivElement | null;
                if (tt) tt.style.display = 'none';
            });
        }

        const { left, top, idx } = u.cursor;

        // Get or create the tooltip element scoped to this chart's root
        let tooltip = u.root.querySelector(`.${TOOLTIP_CLASS}`) as HTMLDivElement | null;

        // Only show tooltip on the chart being directly hovered
        const isHovered = u.root.hasAttribute(HOVER_ATTR);

        if (!isHovered || left == null || left < 0 || idx == null) {
            if (tooltip) {
                tooltip.style.display = 'none';
            }
            return;
        }

        if (!tooltip) {
            tooltip = document.createElement('div');
            tooltip.className = TOOLTIP_CLASS;
            u.root.appendChild(tooltip);
        }

        // Build tooltip entries, grouping fan chart series (median + range)
        const entries: TooltipEntry[] = [];

        for (let i = 1; i < u.series.length; i++) {
            const series = u.series[i];
            if (!series.show) continue;

            const stroke = series.stroke;
            if (!stroke || stroke === 'transparent') continue;
            if ((series as any).width === 0) continue;

            const value = u.data[i]?.[idx];
            if (value === null || value === undefined) continue;

            const label = series.label;
            if (!label || typeof label !== 'string') continue;
            // Skip series whose labels are percentile boundaries
            if (/^(p\d+|.*\bp\d+)$/i.test(label)) continue;

            const colorStr: string = typeof stroke === 'string' ? stroke : 'var(--color-text-primary)';

            // Look for corresponding p10 and p90 values for this series.
            // Fan chart data layout per variable: p90, p75, median, p25, p10
            // So median is at index i, p90 is at i-2, p10 is at i+2
            let p10: number | null | undefined = undefined;
            let p90: number | null | undefined = undefined;

            // Check if this looks like a median in a fan chart group:
            // The series 2 positions before should be labeled with p90/p75 pattern
            // and 2 positions after should be p25/p10 pattern
            if (i >= 3) {
                const prevLabel = u.series[i - 2]?.label;
                const prev2Label = u.series[i - 1]?.label;
                if (typeof prevLabel === 'string' && typeof prev2Label === 'string' &&
                    (/p90/i.test(prevLabel) || (u.series[i - 2] as any)?.width === 0) &&
                    (/p75/i.test(prev2Label) || (u.series[i - 1] as any)?.width === 0)) {
                    // This is likely a median — grab p90 (i-2) and p10 (i+2)
                    const p90Val = u.data[i - 2]?.[idx];
                    const p10Val = u.data[i + 2]?.[idx];
                    if (typeof p90Val === 'number') p90 = p90Val;
                    if (typeof p10Val === 'number') p10 = p10Val;
                }
            }

            entries.push({ label, color: colorStr, value, p10, p90 });
        }

        if (entries.length === 0) {
            tooltip.style.display = 'none';
            return;
        }

        // Build HTML
        const lines: string[] = [];

        // Time label
        const time = u.data[0][idx];
        if (time !== undefined && time !== null) {
            const date = new Date(time * 1000);
            lines.push(`<span class="chart-crosshair-tooltip__time">${date.toLocaleString(undefined, {
                weekday: 'short',
                month: 'short',
                day: 'numeric',
                hour: '2-digit',
                minute: '2-digit',
            })}</span>`);
        }

        for (const entry of entries) {
            let valueStr = `${Math.round(entry.value)}`;
            if (entry.p10 != null && entry.p90 != null) {
                valueStr += `<span class="chart-crosshair-tooltip__range">(${Math.round(entry.p10)}–${Math.round(entry.p90)})</span>`;
            }
            lines.push(`<span class="chart-crosshair-tooltip__row"><span class="chart-crosshair-tooltip__dot" style="background:${entry.color}"></span><span class="chart-crosshair-tooltip__label" style="color:${entry.color}">${entry.label}:</span> <span class="chart-crosshair-tooltip__value" style="color:${entry.color}">${valueStr}</span></span>`);
        }

        tooltip.innerHTML = lines.join('');
        tooltip.style.display = 'flex';

        // Position tooltip relative to u.root.
        // u.cursor.left/top are CSS pixels from the plot area's left/top edge.
        // u.bbox is in canvas pixels, divide by devicePixelRatio for CSS pixels.
        const plotLeft = u.bbox.left / devicePixelRatio;
        const plotTop = u.bbox.top / devicePixelRatio;
        const plotWidth = u.bbox.width / devicePixelRatio;
        const plotHeight = u.bbox.height / devicePixelRatio;

        const tooltipWidth = tooltip.offsetWidth;
        const tooltipHeight = tooltip.offsetHeight;
        const cursorCssX = plotLeft + left;
        const cursorCssY = plotTop + (top ?? 0);

        // Horizontal: flip to left side if too close to right edge
        if (cursorCssX + tooltipWidth + 12 > plotLeft + plotWidth) {
            tooltip.style.left = `${cursorCssX - tooltipWidth - 8}px`;
        } else {
            tooltip.style.left = `${cursorCssX + 12}px`;
        }

        // Vertical: position near cursor, but clamp within plot area
        let tooltipY = cursorCssY - tooltipHeight / 2;
        tooltipY = Math.max(plotTop, Math.min(tooltipY, plotTop + plotHeight - tooltipHeight));
        tooltip.style.top = `${tooltipY}px`;
    };
}
