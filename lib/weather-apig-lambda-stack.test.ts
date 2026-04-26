import * as cdk from 'aws-cdk-lib';
import { Template, Match } from 'aws-cdk-lib/assertions';
import { WeatherApigLambdaStack } from './weather-apig-lambda-stack';
import * as fs from 'fs';
import * as path from 'path';

// The CDK stack references Lambda code from target/lambda/*.zip files.
// We create dummy zip files so the stack can synthesize during tests.
const LAMBDA_DIR = path.join(__dirname, '..', 'target', 'lambda');
const LAMBDA_NAMES = ['forecast', 'geocode', 'metadata', 'stations'];

function createDummyZips(): void {
    fs.mkdirSync(LAMBDA_DIR, { recursive: true });

    // A minimal valid zip file (empty zip archive: PK\x05\x06 + 18 zero bytes)
    const emptyZip = Buffer.from([
        0x50, 0x4b, 0x05, 0x06, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ]);

    for (const name of LAMBDA_NAMES) {
        const zipPath = path.join(LAMBDA_DIR, `${name}.zip`);
        if (!fs.existsSync(zipPath)) {
            fs.writeFileSync(zipPath, emptyZip);
        }
    }
}

function cleanupDummyZips(): void {
    for (const name of LAMBDA_NAMES) {
        const zipPath = path.join(LAMBDA_DIR, `${name}.zip`);
        if (fs.existsSync(zipPath)) {
            fs.unlinkSync(zipPath);
        }
    }
    // Remove the directory only if empty
    try {
        fs.rmdirSync(LAMBDA_DIR);
    } catch {
        // Directory not empty (other files exist), leave it
    }
}

describe('WeatherApigLambdaStack', () => {
    let template: Template;

    beforeAll(() => {
        createDummyZips();

        const app = new cdk.App();
        const stack = new WeatherApigLambdaStack(app, 'TestWeatherStack', {
            env: { region: 'us-west-1', account: '655347895545' },
        });
        template = Template.fromStack(stack);
    });

    afterAll(() => {
        cleanupDummyZips();
    });

    // --- Requirement 1.1: API Gateway REST API ---

    test('creates an API Gateway REST API', () => {
        template.hasResourceProperties('AWS::ApiGateway::RestApi', {
            Name: 'WeatherApi',
        });
    });

    // --- Requirement 1.2: Route 53 A record ---

    test('creates a Route 53 A record', () => {
        template.hasResourceProperties('AWS::Route53::RecordSet', {
            Type: 'A',
            Name: 'weather.popelka-woods.com.',
        });
    });

    // --- Requirement 1.3, 1.10: Lambda functions use provided.al2023 runtime and ARM64 ---

    test('all Lambda functions use provided.al2023 runtime and ARM64 architecture', () => {
        const lambdas = template.findResources('AWS::Lambda::Function');
        const lambdaLogicalIds = Object.keys(lambdas);

        // 4 application Lambdas + 1 CDK auto-delete-objects custom resource Lambda
        expect(lambdaLogicalIds.length).toBeGreaterThanOrEqual(4);

        // Filter to only our application Lambdas (runtime = provided.al2023)
        const appLambdas = lambdaLogicalIds.filter(
            (id) => lambdas[id].Properties.Runtime === 'provided.al2023'
        );
        expect(appLambdas.length).toBe(4);

        for (const logicalId of appLambdas) {
            const properties = lambdas[logicalId].Properties;
            expect(properties.Runtime).toBe('provided.al2023');
            expect(properties.Architectures).toEqual(['arm64']);
        }
    });

    // --- Requirement 1.5, 1.6: ForecastLambda has ≥512MB memory and ≥30s timeout ---

    test('ForecastLambda has at least 512MB memory and at least 30s timeout', () => {
        // The stack sets exactly 512MB and 30s. We verify these meet the
        // ≥512MB and ≥30s requirements by asserting the concrete values.
        template.hasResourceProperties('AWS::Lambda::Function', {
            MemorySize: 512,
            Timeout: 30,
            Handler: 'bootstrap',
        });
    });

    // --- Requirement 1.4: DynamoDB table has correct key schema and TTL ---

    test('DynamoDB table has correct key schema and TTL attribute', () => {
        template.hasResourceProperties('AWS::DynamoDB::Table', {
            KeySchema: Match.arrayWith([
                Match.objectLike({ AttributeName: 'cache_key', KeyType: 'HASH' }),
                Match.objectLike({ AttributeName: 'source', KeyType: 'RANGE' }),
            ]),
            TimeToLiveSpecification: {
                AttributeName: 'expires_at',
                Enabled: true,
            },
        });
    });

    // --- Requirement 1.4: S3 bucket exists with lifecycle rules ---

    test('S3 bucket exists with lifecycle rules', () => {
        template.hasResourceProperties('AWS::S3::Bucket', {
            LifecycleConfiguration: {
                Rules: Match.arrayWith([
                    Match.objectLike({
                        ExpirationInDays: Match.anyValue(),
                        Status: 'Enabled',
                    }),
                ]),
            },
        });
    });

    // --- Requirement 1.7: CORS configuration allows all origins ---

    test('CORS configuration allows all origins', () => {
        // CDK's defaultCorsPreflightOptions creates OPTIONS methods with
        // MOCK integration that return Access-Control-Allow-Origin: '*'.
        template.hasResourceProperties('AWS::ApiGateway::Method', {
            HttpMethod: 'OPTIONS',
            Integration: Match.objectLike({
                Type: 'MOCK',
                IntegrationResponses: Match.arrayWith([
                    Match.objectLike({
                        ResponseParameters: Match.objectLike({
                            'method.response.header.Access-Control-Allow-Origin': "'*'",
                        }),
                    }),
                ]),
            }),
        });
    });

    // --- Requirement 1.8: CloudWatch log group has retention policy (1 week = 7 days) ---

    test('CloudWatch log group has 7-day retention policy', () => {
        template.hasResourceProperties('AWS::Logs::LogGroup', {
            RetentionInDays: 7,
        });
    });
});
