export type TempUnit = 'C' | 'F';
export type WindUnit = 'kmh' | 'mph' | 'kts' | 'ms';
export type PressureUnit = 'hPa' | 'inHg' | 'mmHg';
export type PrecipUnit = 'mm' | 'in';
export type WaveUnit = 'm' | 'ft';

export interface UnitPreferences {
    temperature: TempUnit;
    wind: WindUnit;
    pressure: PressureUnit;
    precipitation: PrecipUnit;
    wave: WaveUnit;
}
