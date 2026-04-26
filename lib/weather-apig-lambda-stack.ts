import * as cdk from 'aws-cdk-lib';
import { Construct } from 'constructs';
import * as lambda from 'aws-cdk-lib/aws-lambda';
import * as apigateway from 'aws-cdk-lib/aws-apigateway';
import { aws_dynamodb as ddb } from 'aws-cdk-lib';
import { aws_route53 as route53 } from 'aws-cdk-lib';
import { aws_certificatemanager as acm } from 'aws-cdk-lib';
import { aws_route53_targets as targets } from 'aws-cdk-lib';
import * as logs from 'aws-cdk-lib/aws-logs';
import * as iam from 'aws-cdk-lib/aws-iam';
import * as s3 from 'aws-cdk-lib/aws-s3';

export class WeatherApigLambdaStack extends cdk.Stack {
    constructor(scope: Construct, id: string, props?: cdk.StackProps) {
        super(scope, id, props);

        // --- DNS & Certificate ---

        const hostedZone = route53.HostedZone.fromHostedZoneAttributes(this, 'PopelkaWoodsZone', {
            zoneName: 'popelka-woods.com',
            hostedZoneId: 'Z0729656RCDIV32E8RF3',
        });

        const certificate = new acm.Certificate(this, 'WeatherCert', {
            domainName: 'weather.popelka-woods.com',
            validation: acm.CertificateValidation.fromDns(hostedZone),
        });

        // --- S3 Bucket for Cache ---

        const cacheBucket = new s3.Bucket(this, 'WeatherCacheBucket', {
            bucketName: 'weather-cache',
            removalPolicy: cdk.RemovalPolicy.DESTROY,
            autoDeleteObjects: true,
            lifecycleRules: [{
                expiration: cdk.Duration.days(1),
            }],
        });

        // --- DynamoDB Table for Cache ---

        const cacheTable = new ddb.Table(this, 'WeatherCacheTable', {
            tableName: 'weather-cache',
            partitionKey: { name: 'cache_key', type: ddb.AttributeType.STRING },
            sortKey: { name: 'source', type: ddb.AttributeType.STRING },
            timeToLiveAttribute: 'expires_at',
            billingMode: ddb.BillingMode.PAY_PER_REQUEST,
            removalPolicy: cdk.RemovalPolicy.DESTROY,
        });

        // --- Lambda Functions ---

        const forecastLambda = new lambda.Function(this, 'ForecastLambda', {
            runtime: lambda.Runtime.PROVIDED_AL2023,
            architecture: lambda.Architecture.ARM_64,
            handler: 'bootstrap',
            code: lambda.Code.fromAsset('target/lambda/forecast.zip'),
            memorySize: 512,
            timeout: cdk.Duration.seconds(30),
            environment: {
                CACHE_BUCKET: cacheBucket.bucketName,
                CACHE_TABLE: cacheTable.tableName,
            },
        });

        const geocodeLambda = new lambda.Function(this, 'GeocodeLambda', {
            runtime: lambda.Runtime.PROVIDED_AL2023,
            architecture: lambda.Architecture.ARM_64,
            handler: 'bootstrap',
            code: lambda.Code.fromAsset('target/lambda/geocode.zip'),
            memorySize: 128,
            timeout: cdk.Duration.seconds(10),
        });

        const metadataLambda = new lambda.Function(this, 'MetadataLambda', {
            runtime: lambda.Runtime.PROVIDED_AL2023,
            architecture: lambda.Architecture.ARM_64,
            handler: 'bootstrap',
            code: lambda.Code.fromAsset('target/lambda/metadata.zip'),
            memorySize: 128,
            timeout: cdk.Duration.seconds(15),
        });

        const stationsLambda = new lambda.Function(this, 'StationsLambda', {
            runtime: lambda.Runtime.PROVIDED_AL2023,
            architecture: lambda.Architecture.ARM_64,
            handler: 'bootstrap',
            code: lambda.Code.fromAsset('target/lambda/stations.zip'),
            memorySize: 128,
            timeout: cdk.Duration.seconds(10),
        });

        // --- Permissions ---

        cacheBucket.grantReadWrite(forecastLambda);
        cacheTable.grantReadWriteData(forecastLambda);

        // --- API Gateway CloudWatch Logging ---
        // Required since @aws-cdk/aws-apigateway:disableCloudWatchRole is true

        const apiGatewayCloudWatchRole = new iam.Role(this, 'ApiGatewayCloudWatchRole', {
            assumedBy: new iam.ServicePrincipal('apigateway.amazonaws.com'),
            managedPolicies: [
                iam.ManagedPolicy.fromAwsManagedPolicyName('service-role/AmazonAPIGatewayPushToCloudWatchLogs'),
            ],
        });

        new apigateway.CfnAccount(this, 'ApiGatewayAccount', {
            cloudWatchRoleArn: apiGatewayCloudWatchRole.roleArn,
        });

        const apiLogGroup = new logs.LogGroup(this, 'WeatherApiLogs', {
            retention: logs.RetentionDays.ONE_WEEK,
        });

        // --- API Gateway ---

        const api = new apigateway.RestApi(this, 'WeatherApi', {
            restApiName: 'WeatherApi',
            description: 'Weather API — forecast, geocode, metadata, stations',
            domainName: {
                domainName: 'weather.popelka-woods.com',
                certificate,
                endpointType: apigateway.EndpointType.REGIONAL,
                securityPolicy: apigateway.SecurityPolicy.TLS_1_2,
            },
            deployOptions: {
                loggingLevel: apigateway.MethodLoggingLevel.ERROR,
                dataTraceEnabled: true,
                accessLogDestination: new apigateway.LogGroupLogDestination(apiLogGroup),
                accessLogFormat: apigateway.AccessLogFormat.jsonWithStandardFields({
                    httpMethod: true,
                    ip: true,
                    protocol: true,
                    requestTime: true,
                    resourcePath: true,
                    responseLength: true,
                    status: true,
                    caller: true,
                    user: true,
                }),
            },
            defaultCorsPreflightOptions: {
                allowOrigins: apigateway.Cors.ALL_ORIGINS,
                allowMethods: apigateway.Cors.ALL_METHODS,
                allowHeaders: ['Content-Type', 'Authorization'],
            },
        });

        // --- Route 53 A Record ---

        new route53.ARecord(this, 'WeatherApiAliasRecord', {
            zone: hostedZone,
            recordName: 'weather.popelka-woods.com',
            target: route53.RecordTarget.fromAlias(new targets.ApiGateway(api)),
        });

        // --- API Gateway Routes ---

        const forecastIntegration = new apigateway.LambdaIntegration(forecastLambda);
        const geocodeIntegration = new apigateway.LambdaIntegration(geocodeLambda);
        const metadataIntegration = new apigateway.LambdaIntegration(metadataLambda);
        const stationsIntegration = new apigateway.LambdaIntegration(stationsLambda);

        // GET /forecast → ForecastLambda
        const forecast = api.root.addResource('forecast');
        forecast.addMethod('GET', forecastIntegration);

        // GET /geocode → GeocodeLambda
        const geocode = api.root.addResource('geocode');
        geocode.addMethod('GET', geocodeIntegration);

        // GET /models/metadata → MetadataLambda
        const models = api.root.addResource('models');
        const metadata = models.addResource('metadata');
        metadata.addMethod('GET', metadataIntegration);

        // GET /stations/observations → StationsLambda
        // GET /stations/marine → StationsLambda
        const stations = api.root.addResource('stations');
        const observations = stations.addResource('observations');
        observations.addMethod('GET', stationsIntegration);

        const marine = stations.addResource('marine');
        marine.addMethod('GET', stationsIntegration);

        // --- Outputs ---

        new cdk.CfnOutput(this, 'ApiUrl', { value: api.url ?? 'undefined' });
    }
}
