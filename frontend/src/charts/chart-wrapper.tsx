import { useRef, useEffect, useMemo } from "preact/hooks";
import type { FunctionComponent } from "preact";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";

export interface ChartWrapperProps {
    options: uPlot.Options;
    data: uPlot.AlignedData;
    width?: number;
    height?: number;
}

const DEFAULT_HEIGHT = 260;

/**
 * Derive a stable key from the x-axis scale configuration.
 * When this changes (e.g., zoom level change), the chart is recreated.
 * Uses the duration (max - min) rounded to nearest hour to stay stable
 * across re-renders while detecting actual zoom level changes.
 */
function getScaleKey(options: uPlot.Options): string {
    const xScale = options.scales?.x;
    if (!xScale) return 'auto';
    const min = (xScale as any).min;
    const max = (xScale as any).max;
    if (min != null && max != null) {
        // Round duration to nearest hour — zoom levels differ by days
        const durationHours = Math.round((max - min) / 3600);
        return `dur_${durationHours}`;
    }
    return 'auto';
}

export const ChartWrapper: FunctionComponent<ChartWrapperProps> = ({
    options,
    data,
    width,
    height,
}) => {
    const containerRef = useRef<HTMLDivElement>(null);
    const chartRef = useRef<uPlot | null>(null);

    const optionsRef = useRef(options);
    const dataRef = useRef(data);
    optionsRef.current = options;
    dataRef.current = data;

    // Compute a stable scale key that only changes when zoom level changes
    const scaleKey = useMemo(() => getScaleKey(options), [options]);

    // Create or recreate chart when scale key changes
    useEffect(() => {
        const container = containerRef.current;
        if (!container) return;

        // Destroy existing chart if any
        if (chartRef.current) {
            chartRef.current.destroy();
            chartRef.current = null;
        }

        let rafId = requestAnimationFrame(() => {
            const chartWidth = width ?? Math.max(container.clientWidth, 400);
            const chartHeight = height ?? DEFAULT_HEIGHT;

            const opts: uPlot.Options = {
                ...optionsRef.current,
                width: chartWidth,
                height: chartHeight,
            };

            try {
                chartRef.current = new uPlot(opts, dataRef.current, container);
            } catch (e) {
                console.error('[ChartWrapper] uPlot creation failed:', e);
            }
        });

        return () => {
            cancelAnimationFrame(rafId);
            if (chartRef.current) {
                chartRef.current.destroy();
                chartRef.current = null;
            }
        };
    }, [scaleKey]); // Recreate when scale range changes

    // Update data when it changes (without full recreate)
    useEffect(() => {
        if (chartRef.current) {
            chartRef.current.setData(data);
        }
    }, [data]);

    // ResizeObserver for responsive sizing
    useEffect(() => {
        const container = containerRef.current;
        if (!container) return;

        const observer = new ResizeObserver((entries) => {
            const entry = entries[0];
            if (!entry) return;

            const newWidth = width ?? entry.contentRect.width;
            const newHeight = height ?? DEFAULT_HEIGHT;
            if (newWidth <= 0) return;

            if (chartRef.current) {
                chartRef.current.setSize({ width: newWidth, height: newHeight });
            } else {
                // Chart wasn't created yet — create now that we have a real width
                const opts: uPlot.Options = {
                    ...optionsRef.current,
                    width: newWidth,
                    height: newHeight,
                };
                try {
                    chartRef.current = new uPlot(opts, dataRef.current, container);
                } catch (e) {
                    console.error('[ChartWrapper] Deferred uPlot creation failed:', e);
                }
            }
        });

        observer.observe(container);
        return () => observer.disconnect();
    }, [width, height]);

    return <div ref={containerRef} style={{ width: '100%', minHeight: `${height ?? DEFAULT_HEIGHT}px` }} />;
};
