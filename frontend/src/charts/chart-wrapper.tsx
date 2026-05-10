import { useRef, useEffect } from "preact/hooks";
import type { FunctionComponent } from "preact";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";

export interface ChartWrapperProps {
    options: uPlot.Options;
    data: uPlot.AlignedData;
    width?: number;
    height?: number;
}

const DEFAULT_HEIGHT = 200;

export const ChartWrapper: FunctionComponent<ChartWrapperProps> = ({
    options,
    data,
    width,
    height,
}) => {
    const containerRef = useRef<HTMLDivElement>(null);
    const chartRef = useRef<uPlot | null>(null);

    // Create uPlot instance on mount, destroy on unmount
    useEffect(() => {
        const container = containerRef.current;
        if (!container) return;

        const chartWidth = width ?? container.clientWidth;
        const chartHeight = height ?? DEFAULT_HEIGHT;

        const opts: uPlot.Options = {
            ...options,
            width: chartWidth,
            height: chartHeight,
        };

        const chart = new uPlot(opts, data, container);
        chartRef.current = chart;

        return () => {
            chart.destroy();
            chartRef.current = null;
        };
        // Only run on mount/unmount — options identity change means a full rebuild
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [options]);

    // Update data without recreating the instance
    useEffect(() => {
        if (chartRef.current) {
            chartRef.current.setData(data);
        }
    }, [data]);

    // Resize on container dimension changes via ResizeObserver
    useEffect(() => {
        const container = containerRef.current;
        if (!container) return;

        const observer = new ResizeObserver((entries) => {
            const entry = entries[0];
            if (!entry || !chartRef.current) return;

            const newWidth = width ?? entry.contentRect.width;
            const newHeight = height ?? DEFAULT_HEIGHT;
            chartRef.current.setSize({ width: newWidth, height: newHeight });
        });

        observer.observe(container);

        return () => {
            observer.disconnect();
        };
    }, [width, height]);

    return <div ref={containerRef} />;
};
