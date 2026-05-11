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
import * as cloudfront from 'aws-cdk-lib/aws-cloudfront';
import * as origins from 'aws-cdk-lib/aws-cloudfront-origins';
import * as s3deploy from 'aws-cdk-lib/aws-s3-deployment';
import * as events from 'aws-cdk-lib/aws-events';
import * as eventsTargets from 'aws-cdk-lib/aws-events-targets';
import * as cloudwatch from 'aws-cdk-lib/aws-cloudwatch';

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

        // CloudFront requires certificates in us-east-1
        const cdnCertificate = new acm.DnsValidatedCertificate(this, 'WeatherCdnCert', {
            domainName: 'weather.popelka-woods.com',
            hostedZone,
            region: 'us-east-1',
        });

        // --- S3 Bucket for Cache ---

        const cacheBucket = new s3.Bucket(this, 'WeatherCacheBucket', {
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

        // --- DynamoDB Table for Location Tracker ---

        const locationTrackerTable = new ddb.Table(this, 'LocationTrackerTable', {
            tableName: 'weather-location-tracker',
            partitionKey: { name: 'cache_key', type: ddb.AttributeType.STRING },
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
            memorySize: 5308,
            timeout: cdk.Duration.seconds(30),
            environment: {
                CACHE_BUCKET: cacheBucket.bucketName,
                CACHE_TABLE: cacheTable.tableName,
                LOCATION_TRACKER_TABLE: locationTrackerTable.tableName,
                AWS_LAMBDA_HTTP_IGNORE_STAGE_IN_PATH: 'true',
            },
        });

        const geocodeLambda = new lambda.Function(this, 'GeocodeLambda', {
            runtime: lambda.Runtime.PROVIDED_AL2023,
            architecture: lambda.Architecture.ARM_64,
            handler: 'bootstrap',
            code: lambda.Code.fromAsset('target/lambda/geocode.zip'),
            memorySize: 128,
            timeout: cdk.Duration.seconds(10),
            environment: {
                AWS_LAMBDA_HTTP_IGNORE_STAGE_IN_PATH: 'true',
            },
        });

        const metadataLambda = new lambda.Function(this, 'MetadataLambda', {
            runtime: lambda.Runtime.PROVIDED_AL2023,
            architecture: lambda.Architecture.ARM_64,
            handler: 'bootstrap',
            code: lambda.Code.fromAsset('target/lambda/metadata.zip'),
            memorySize: 5308,
            timeout: cdk.Duration.seconds(15),
            environment: {
                CACHE_TABLE: cacheTable.tableName,
                AWS_LAMBDA_HTTP_IGNORE_STAGE_IN_PATH: 'true',
            },
        });

        const stationsLambda = new lambda.Function(this, 'StationsLambda', {
            runtime: lambda.Runtime.PROVIDED_AL2023,
            architecture: lambda.Architecture.ARM_64,
            handler: 'bootstrap',
            code: lambda.Code.fromAsset('target/lambda/stations.zip'),
            memorySize: 128,
            timeout: cdk.Duration.seconds(10),
            environment: {
                AWS_LAMBDA_HTTP_IGNORE_STAGE_IN_PATH: 'true',
            },
        });

        // --- Permissions ---

        cacheBucket.grantReadWrite(forecastLambda);
        cacheTable.grantReadWriteData(forecastLambda);
        locationTrackerTable.grantWriteData(forecastLambda);
        cacheTable.grantReadWriteData(metadataLambda);

        // --- Cache Warmer Lambda ---

        const cacheWarmerLambda = new lambda.Function(this, 'CacheWarmerLambda', {
            runtime: lambda.Runtime.PROVIDED_AL2023,
            architecture: lambda.Architecture.ARM_64,
            handler: 'bootstrap',
            code: lambda.Code.fromAsset('target/lambda/cache_warmer.zip'),
            memorySize: 512,
            timeout: cdk.Duration.seconds(300),
            environment: {
                LOCATION_TRACKER_TABLE: locationTrackerTable.tableName,
                CACHE_BUCKET: cacheBucket.bucketName,
                CACHE_TABLE: cacheTable.tableName,
            },
        });

        locationTrackerTable.grantReadData(cacheWarmerLambda);
        cacheBucket.grantReadWrite(cacheWarmerLambda);
        cacheTable.grantReadWriteData(cacheWarmerLambda);

        // --- EventBridge Schedule for Cache Warmer ---

        const cacheWarmerSchedule = new events.Rule(this, 'CacheWarmerSchedule', {
            schedule: events.Schedule.rate(cdk.Duration.minutes(30)),
        });
        cacheWarmerSchedule.addTarget(new eventsTargets.LambdaFunction(cacheWarmerLambda));

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
                metricsEnabled: true,
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
                allowHeaders: ['Content-Type', 'Authorization', 'x-api-key'],
            },
        });

        // --- API Key & Usage Plan ---
        // The key value is specified explicitly so it can be referenced in the
        // CloudFront origin custom headers (CloudFront injects it on every request
        // to API Gateway, so the frontend never needs to send it).
        const apiKeyValue = 'WeatherApiKey2025PopelkaWoods';

        const apiKey = api.addApiKey('WeatherApiKey', {
            apiKeyName: 'weather-api-key-cf',
            description: 'API key for weather.popelka-woods.com (injected by CloudFront)',
            value: apiKeyValue,
        });

        const usagePlan = api.addUsagePlan('WeatherUsagePlan', {
            name: 'WeatherUsagePlan',
            description: 'Usage plan for the Weather API',
            apiStages: [{ api, stage: api.deploymentStage }],
        });

        usagePlan.addApiKey(apiKey);

        // --- API Gateway Routes ---

        const forecastIntegration = new apigateway.LambdaIntegration(forecastLambda);
        const geocodeIntegration = new apigateway.LambdaIntegration(geocodeLambda);
        const metadataIntegration = new apigateway.LambdaIntegration(metadataLambda);
        const stationsIntegration = new apigateway.LambdaIntegration(stationsLambda);

        // GET /forecast → ForecastLambda
        const forecast = api.root.addResource('forecast');
        forecast.addMethod('GET', forecastIntegration, { apiKeyRequired: true });

        // GET /forecast/members → ForecastLambda
        const forecastMembers = forecast.addResource('members');
        forecastMembers.addMethod('GET', forecastIntegration, { apiKeyRequired: true });

        // GET /geocode → GeocodeLambda
        const geocode = api.root.addResource('geocode');
        geocode.addMethod('GET', geocodeIntegration, { apiKeyRequired: true });

        // GET /models/metadata → MetadataLambda
        const models = api.root.addResource('models');
        const metadata = models.addResource('metadata');
        metadata.addMethod('GET', metadataIntegration, { apiKeyRequired: true });

        // GET /stations/observations → StationsLambda
        // GET /stations/marine → StationsLambda
        const stations = api.root.addResource('stations');
        const observations = stations.addResource('observations');
        observations.addMethod('GET', stationsIntegration, { apiKeyRequired: true });

        const marine = stations.addResource('marine');
        marine.addMethod('GET', stationsIntegration, { apiKeyRequired: true });

        // --- S3 Bucket for Frontend Static Assets ---

        const frontendBucket = new s3.Bucket(this, 'WeatherFrontendBucket', {
            removalPolicy: cdk.RemovalPolicy.DESTROY,
            autoDeleteObjects: true,
        });

        // --- CloudFront Distribution ---

        const apiOrigin = new origins.HttpOrigin(
            `${api.restApiId}.execute-api.${this.region}.amazonaws.com`,
            {
                originPath: `/${api.deploymentStage.stageName}`,
                protocolPolicy: cloudfront.OriginProtocolPolicy.HTTPS_ONLY,
                customHeaders: {
                    'x-api-key': apiKeyValue,
                },
            },
        );

        // Use the built-in CACHING_DISABLED policy for API behaviors.
        // The x-api-key header is forwarded via the origin request policy
        // (ALL_VIEWER_EXCEPT_HOST_HEADER forwards all viewer headers to the origin).
        const apiCachePolicy = cloudfront.CachePolicy.CACHING_DISABLED;

        // --- CloudFront Function for SPA routing ---
        // Rewrites requests to /index.html for paths that don't look like files.
        // This replaces the errorResponses approach which interfered with API 403/404 responses.
        const spaRoutingFunction = new cloudfront.Function(this, 'SpaRoutingFunction', {
            code: cloudfront.FunctionCode.fromInline(`
function handler(event) {
    var request = event.request;
    var uri = request.uri;
    // If the URI has a file extension (e.g. .js, .css, .html, .png), pass through
    if (uri.includes('.')) {
        return request;
    }
    // Otherwise rewrite to /index.html for SPA routing
    request.uri = '/index.html';
    return request;
}
`),
            functionName: 'weather-spa-routing',
        });

        const distribution = new cloudfront.Distribution(this, 'WeatherCdn', {
            defaultBehavior: {
                origin: origins.S3BucketOrigin.withOriginAccessControl(frontendBucket),
                viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
                cachePolicy: cloudfront.CachePolicy.CACHING_OPTIMIZED,
                functionAssociations: [{
                    function: spaRoutingFunction,
                    eventType: cloudfront.FunctionEventType.VIEWER_REQUEST,
                }],
            },
            additionalBehaviors: {
                '/forecast': {
                    origin: apiOrigin,
                    viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.HTTPS_ONLY,
                    allowedMethods: cloudfront.AllowedMethods.ALLOW_ALL,
                    cachePolicy: apiCachePolicy,
                    originRequestPolicy: cloudfront.OriginRequestPolicy.ALL_VIEWER_EXCEPT_HOST_HEADER,
                },
                '/forecast/*': {
                    origin: apiOrigin,
                    viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.HTTPS_ONLY,
                    allowedMethods: cloudfront.AllowedMethods.ALLOW_ALL,
                    cachePolicy: apiCachePolicy,
                    originRequestPolicy: cloudfront.OriginRequestPolicy.ALL_VIEWER_EXCEPT_HOST_HEADER,
                },
                '/geocode': {
                    origin: apiOrigin,
                    viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.HTTPS_ONLY,
                    allowedMethods: cloudfront.AllowedMethods.ALLOW_ALL,
                    cachePolicy: apiCachePolicy,
                    originRequestPolicy: cloudfront.OriginRequestPolicy.ALL_VIEWER_EXCEPT_HOST_HEADER,
                },
                '/geocode/*': {
                    origin: apiOrigin,
                    viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.HTTPS_ONLY,
                    allowedMethods: cloudfront.AllowedMethods.ALLOW_ALL,
                    cachePolicy: apiCachePolicy,
                    originRequestPolicy: cloudfront.OriginRequestPolicy.ALL_VIEWER_EXCEPT_HOST_HEADER,
                },
                '/models/*': {
                    origin: apiOrigin,
                    viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.HTTPS_ONLY,
                    allowedMethods: cloudfront.AllowedMethods.ALLOW_ALL,
                    cachePolicy: apiCachePolicy,
                    originRequestPolicy: cloudfront.OriginRequestPolicy.ALL_VIEWER_EXCEPT_HOST_HEADER,
                },
                '/stations/*': {
                    origin: apiOrigin,
                    viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.HTTPS_ONLY,
                    allowedMethods: cloudfront.AllowedMethods.ALLOW_ALL,
                    cachePolicy: apiCachePolicy,
                    originRequestPolicy: cloudfront.OriginRequestPolicy.ALL_VIEWER_EXCEPT_HOST_HEADER,
                },
            },
            domainNames: ['weather.popelka-woods.com'],
            certificate: cdnCertificate,
            defaultRootObject: 'index.html',
        });

        // --- Route 53 A Record ---

        new route53.ARecord(this, 'WeatherApiAliasRecord', {
            zone: hostedZone,
            recordName: 'weather.popelka-woods.com',
            target: route53.RecordTarget.fromAlias(new targets.CloudFrontTarget(distribution)),
        });

        // --- Frontend Deployment ---

        new s3deploy.BucketDeployment(this, 'DeployFrontend', {
            sources: [s3deploy.Source.asset('frontend/dist')],
            destinationBucket: frontendBucket,
            distribution,
            distributionPaths: ['/*'],
            cacheControl: [
                s3deploy.CacheControl.setPublic(),
                s3deploy.CacheControl.maxAge(cdk.Duration.days(365)),
                s3deploy.CacheControl.immutable(),
            ],
        });

        // --- CloudWatch Dashboard ---

        const defaultPeriod = cdk.Duration.seconds(300);

        const dashboard = new cloudwatch.Dashboard(this, 'WeatherApiDashboard', {
            dashboardName: 'WeatherApi-Performance',
            defaultInterval: cdk.Duration.hours(3),
        });

        // --- Lambda Latencies Section ---
        dashboard.addWidgets(
            new cloudwatch.TextWidget({ markdown: '# Lambda Latencies', width: 24, height: 1 }),
        );

        dashboard.addWidgets(
            new cloudwatch.GraphWidget({
                title: 'Forecast Lambda Duration',
                left: [
                    forecastLambda.metricDuration({ statistic: 'p50', period: defaultPeriod }),
                    forecastLambda.metricDuration({ statistic: 'p90', period: defaultPeriod }),
                    forecastLambda.metricDuration({ statistic: 'p99', period: defaultPeriod }),
                ],
                width: 8,
                period: defaultPeriod,
            }),
            new cloudwatch.GraphWidget({
                title: 'Metadata Lambda Duration',
                left: [
                    metadataLambda.metricDuration({ statistic: 'p50', period: defaultPeriod }),
                    metadataLambda.metricDuration({ statistic: 'p90', period: defaultPeriod }),
                    metadataLambda.metricDuration({ statistic: 'p99', period: defaultPeriod }),
                ],
                width: 8,
                period: defaultPeriod,
            }),
            new cloudwatch.GraphWidget({
                title: 'Geocode & Stations Duration',
                left: [
                    geocodeLambda.metricDuration({ statistic: 'p50', label: 'Geocode p50', period: defaultPeriod }),
                    geocodeLambda.metricDuration({ statistic: 'p90', label: 'Geocode p90', period: defaultPeriod }),
                    geocodeLambda.metricDuration({ statistic: 'p99', label: 'Geocode p99', period: defaultPeriod }),
                    stationsLambda.metricDuration({ statistic: 'p50', label: 'Stations p50', period: defaultPeriod }),
                    stationsLambda.metricDuration({ statistic: 'p90', label: 'Stations p90', period: defaultPeriod }),
                    stationsLambda.metricDuration({ statistic: 'p99', label: 'Stations p99', period: defaultPeriod }),
                ],
                width: 8,
                period: defaultPeriod,
            }),
        );

        dashboard.addWidgets(
            new cloudwatch.GraphWidget({
                title: 'Lambda Invocations & Errors',
                left: [
                    forecastLambda.metricInvocations({ label: 'Forecast', period: defaultPeriod }),
                    metadataLambda.metricInvocations({ label: 'Metadata', period: defaultPeriod }),
                    geocodeLambda.metricInvocations({ label: 'Geocode', period: defaultPeriod }),
                    stationsLambda.metricInvocations({ label: 'Stations', period: defaultPeriod }),
                ],
                right: [
                    forecastLambda.metricErrors({ label: 'Forecast Errors', period: defaultPeriod }),
                    metadataLambda.metricErrors({ label: 'Metadata Errors', period: defaultPeriod }),
                    geocodeLambda.metricErrors({ label: 'Geocode Errors', period: defaultPeriod }),
                    stationsLambda.metricErrors({ label: 'Stations Errors', period: defaultPeriod }),
                ],
                width: 24,
                period: defaultPeriod,
            }),
        );

        // --- Forecast Cache Hits Section ---
        const cacheMetricNamespace = 'WeatherApi/Cache';

        dashboard.addWidgets(
            new cloudwatch.TextWidget({ markdown: '# Forecast Cache Hits', width: 24, height: 1 }),
        );

        dashboard.addWidgets(
            new cloudwatch.GraphWidget({
                title: 'Forecast Cache Outcomes',
                left: [
                    new cloudwatch.Metric({ namespace: cacheMetricNamespace, metricName: 'CacheOutcome', dimensionsMap: { CacheType: 'forecast', Outcome: 'full_hit' }, statistic: 'Sum', label: 'Full Hit', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: cacheMetricNamespace, metricName: 'CacheOutcome', dimensionsMap: { CacheType: 'forecast', Outcome: 'partial_hit' }, statistic: 'Sum', label: 'Partial Hit', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: cacheMetricNamespace, metricName: 'CacheOutcome', dimensionsMap: { CacheType: 'forecast', Outcome: 'miss' }, statistic: 'Sum', label: 'Miss', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: cacheMetricNamespace, metricName: 'CacheOutcome', dimensionsMap: { CacheType: 'forecast', Outcome: 'bypass' }, statistic: 'Sum', label: 'Bypass', period: defaultPeriod }),
                ],
                width: 24,
                period: defaultPeriod,
            }),
        );

        // --- Metadata Cache Hits Section ---
        dashboard.addWidgets(
            new cloudwatch.TextWidget({ markdown: '# Metadata Cache Hits', width: 24, height: 1 }),
        );

        dashboard.addWidgets(
            new cloudwatch.GraphWidget({
                title: 'Metadata Cache Outcomes',
                left: [
                    new cloudwatch.Metric({ namespace: cacheMetricNamespace, metricName: 'CacheOutcome', dimensionsMap: { CacheType: 'metadata', Outcome: 'hit' }, statistic: 'Sum', label: 'Hit', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: cacheMetricNamespace, metricName: 'CacheOutcome', dimensionsMap: { CacheType: 'metadata', Outcome: 'miss' }, statistic: 'Sum', label: 'Miss', period: defaultPeriod }),
                ],
                width: 24,
                period: defaultPeriod,
            }),
        );

        // --- Cache Sizes Section ---
        dashboard.addWidgets(
            new cloudwatch.TextWidget({ markdown: '# Cache Sizes', width: 24, height: 1 }),
        );

        dashboard.addWidgets(
            new cloudwatch.SingleValueWidget({
                title: 'S3 Cache Bucket Size',
                metrics: [new cloudwatch.Metric({
                    namespace: 'AWS/S3',
                    metricName: 'BucketSizeBytes',
                    dimensionsMap: { BucketName: cacheBucket.bucketName, StorageType: 'StandardStorage' },
                    statistic: 'Average',
                    period: cdk.Duration.days(1),
                })],
                width: 8,
            }),
            new cloudwatch.SingleValueWidget({
                title: 'S3 Cache Object Count',
                metrics: [new cloudwatch.Metric({
                    namespace: 'AWS/S3',
                    metricName: 'NumberOfObjects',
                    dimensionsMap: { BucketName: cacheBucket.bucketName, StorageType: 'AllStorageTypes' },
                    statistic: 'Average',
                    period: cdk.Duration.days(1),
                })],
                width: 8,
            }),
            new cloudwatch.GraphWidget({
                title: 'DynamoDB Cache Table Capacity',
                left: [
                    cacheTable.metricConsumedReadCapacityUnits({ label: 'Read CU', period: defaultPeriod }),
                    cacheTable.metricConsumedWriteCapacityUnits({ label: 'Write CU', period: defaultPeriod }),
                ],
                width: 8,
                period: defaultPeriod,
            }),
        );

        // --- API Gateway Latencies Section ---
        const apiGwNamespace = 'AWS/ApiGateway';
        const apiName = api.restApiName;

        dashboard.addWidgets(
            new cloudwatch.TextWidget({ markdown: '# API Gateway Latencies', width: 24, height: 1 }),
        );

        dashboard.addWidgets(
            new cloudwatch.GraphWidget({
                title: 'API Gateway IntegrationLatency (p50/p90/p99)',
                left: [
                    new cloudwatch.Metric({ namespace: apiGwNamespace, metricName: 'IntegrationLatency', dimensionsMap: { ApiName: apiName, Resource: '/forecast', Method: 'GET', Stage: 'prod' }, statistic: 'p50', label: '/forecast p50', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: apiGwNamespace, metricName: 'IntegrationLatency', dimensionsMap: { ApiName: apiName, Resource: '/forecast', Method: 'GET', Stage: 'prod' }, statistic: 'p90', label: '/forecast p90', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: apiGwNamespace, metricName: 'IntegrationLatency', dimensionsMap: { ApiName: apiName, Resource: '/forecast', Method: 'GET', Stage: 'prod' }, statistic: 'p99', label: '/forecast p99', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: apiGwNamespace, metricName: 'IntegrationLatency', dimensionsMap: { ApiName: apiName, Resource: '/models/metadata', Method: 'GET', Stage: 'prod' }, statistic: 'p50', label: '/metadata p50', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: apiGwNamespace, metricName: 'IntegrationLatency', dimensionsMap: { ApiName: apiName, Resource: '/models/metadata', Method: 'GET', Stage: 'prod' }, statistic: 'p90', label: '/metadata p90', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: apiGwNamespace, metricName: 'IntegrationLatency', dimensionsMap: { ApiName: apiName, Resource: '/models/metadata', Method: 'GET', Stage: 'prod' }, statistic: 'p99', label: '/metadata p99', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: apiGwNamespace, metricName: 'IntegrationLatency', dimensionsMap: { ApiName: apiName, Resource: '/geocode', Method: 'GET', Stage: 'prod' }, statistic: 'p50', label: '/geocode p50', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: apiGwNamespace, metricName: 'IntegrationLatency', dimensionsMap: { ApiName: apiName, Resource: '/geocode', Method: 'GET', Stage: 'prod' }, statistic: 'p90', label: '/geocode p90', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: apiGwNamespace, metricName: 'IntegrationLatency', dimensionsMap: { ApiName: apiName, Resource: '/geocode', Method: 'GET', Stage: 'prod' }, statistic: 'p99', label: '/geocode p99', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: apiGwNamespace, metricName: 'IntegrationLatency', dimensionsMap: { ApiName: apiName, Resource: '/stations/observations', Method: 'GET', Stage: 'prod' }, statistic: 'p50', label: '/stations p50', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: apiGwNamespace, metricName: 'IntegrationLatency', dimensionsMap: { ApiName: apiName, Resource: '/stations/observations', Method: 'GET', Stage: 'prod' }, statistic: 'p90', label: '/stations p90', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: apiGwNamespace, metricName: 'IntegrationLatency', dimensionsMap: { ApiName: apiName, Resource: '/stations/observations', Method: 'GET', Stage: 'prod' }, statistic: 'p99', label: '/stations p99', period: defaultPeriod }),
                ],
                width: 12,
                period: defaultPeriod,
            }),
            new cloudwatch.GraphWidget({
                title: 'API Gateway Latency — End-to-End (p50/p90/p99)',
                left: [
                    new cloudwatch.Metric({ namespace: apiGwNamespace, metricName: 'Latency', dimensionsMap: { ApiName: apiName, Resource: '/forecast', Method: 'GET', Stage: 'prod' }, statistic: 'p50', label: '/forecast p50', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: apiGwNamespace, metricName: 'Latency', dimensionsMap: { ApiName: apiName, Resource: '/forecast', Method: 'GET', Stage: 'prod' }, statistic: 'p90', label: '/forecast p90', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: apiGwNamespace, metricName: 'Latency', dimensionsMap: { ApiName: apiName, Resource: '/forecast', Method: 'GET', Stage: 'prod' }, statistic: 'p99', label: '/forecast p99', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: apiGwNamespace, metricName: 'Latency', dimensionsMap: { ApiName: apiName, Resource: '/models/metadata', Method: 'GET', Stage: 'prod' }, statistic: 'p50', label: '/metadata p50', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: apiGwNamespace, metricName: 'Latency', dimensionsMap: { ApiName: apiName, Resource: '/models/metadata', Method: 'GET', Stage: 'prod' }, statistic: 'p90', label: '/metadata p90', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: apiGwNamespace, metricName: 'Latency', dimensionsMap: { ApiName: apiName, Resource: '/models/metadata', Method: 'GET', Stage: 'prod' }, statistic: 'p99', label: '/metadata p99', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: apiGwNamespace, metricName: 'Latency', dimensionsMap: { ApiName: apiName, Resource: '/geocode', Method: 'GET', Stage: 'prod' }, statistic: 'p50', label: '/geocode p50', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: apiGwNamespace, metricName: 'Latency', dimensionsMap: { ApiName: apiName, Resource: '/geocode', Method: 'GET', Stage: 'prod' }, statistic: 'p90', label: '/geocode p90', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: apiGwNamespace, metricName: 'Latency', dimensionsMap: { ApiName: apiName, Resource: '/geocode', Method: 'GET', Stage: 'prod' }, statistic: 'p99', label: '/geocode p99', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: apiGwNamespace, metricName: 'Latency', dimensionsMap: { ApiName: apiName, Resource: '/stations/observations', Method: 'GET', Stage: 'prod' }, statistic: 'p50', label: '/stations p50', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: apiGwNamespace, metricName: 'Latency', dimensionsMap: { ApiName: apiName, Resource: '/stations/observations', Method: 'GET', Stage: 'prod' }, statistic: 'p90', label: '/stations p90', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: apiGwNamespace, metricName: 'Latency', dimensionsMap: { ApiName: apiName, Resource: '/stations/observations', Method: 'GET', Stage: 'prod' }, statistic: 'p99', label: '/stations p99', period: defaultPeriod }),
                ],
                width: 12,
                period: defaultPeriod,
            }),
        );

        // --- Cache Warmer Section ---
        const warmerNamespace = 'WeatherApi/CacheWarmer';

        dashboard.addWidgets(
            new cloudwatch.TextWidget({ markdown: '# Cache Warmer', width: 24, height: 1 }),
        );

        dashboard.addWidgets(
            new cloudwatch.GraphWidget({
                title: 'Warmer Run Duration',
                left: [
                    new cloudwatch.Metric({ namespace: warmerNamespace, metricName: 'ElapsedMs', dimensionsMap: { Metric: 'RunSummary' }, statistic: 'Maximum', label: 'Duration (ms)', period: defaultPeriod }),
                ],
                width: 8,
                period: defaultPeriod,
            }),
            new cloudwatch.GraphWidget({
                title: 'Sources Checked vs Refreshed vs Skipped',
                left: [
                    new cloudwatch.Metric({ namespace: warmerNamespace, metricName: 'SourcesChecked', dimensionsMap: { Metric: 'RunSummary' }, statistic: 'Sum', label: 'Checked', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: warmerNamespace, metricName: 'SourcesRefreshed', dimensionsMap: { Metric: 'RunSummary' }, statistic: 'Sum', label: 'Refreshed', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: warmerNamespace, metricName: 'SourcesSkipped', dimensionsMap: { Metric: 'RunSummary' }, statistic: 'Sum', label: 'Skipped (still fresh)', period: defaultPeriod }),
                ],
                width: 8,
                period: defaultPeriod,
            }),
            new cloudwatch.GraphWidget({
                title: 'Locations & Errors',
                left: [
                    new cloudwatch.Metric({ namespace: warmerNamespace, metricName: 'LocationsFound', dimensionsMap: { Metric: 'RunSummary' }, statistic: 'Maximum', label: 'Locations Found', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: warmerNamespace, metricName: 'LocationsProcessed', dimensionsMap: { Metric: 'RunSummary' }, statistic: 'Maximum', label: 'Locations Processed', period: defaultPeriod }),
                ],
                right: [
                    new cloudwatch.Metric({ namespace: warmerNamespace, metricName: 'Errors', dimensionsMap: { Metric: 'RunSummary' }, statistic: 'Sum', label: 'Errors', period: defaultPeriod }),
                ],
                width: 8,
                period: defaultPeriod,
            }),
        );

        dashboard.addWidgets(
            new cloudwatch.GraphWidget({
                title: 'Upstream Fetch Latency by Source (p50/p90)',
                left: [
                    new cloudwatch.Metric({ namespace: warmerNamespace, metricName: 'FetchLatency', dimensionsMap: { Source: 'marine', Outcome: 'success' }, statistic: 'p50', label: 'marine p50', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: warmerNamespace, metricName: 'FetchLatency', dimensionsMap: { Source: 'marine', Outcome: 'success' }, statistic: 'p90', label: 'marine p90', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: warmerNamespace, metricName: 'FetchLatency', dimensionsMap: { Source: 'hrrr', Outcome: 'success' }, statistic: 'p50', label: 'hrrr p50', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: warmerNamespace, metricName: 'FetchLatency', dimensionsMap: { Source: 'hrrr', Outcome: 'success' }, statistic: 'p90', label: 'hrrr p90', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: warmerNamespace, metricName: 'FetchLatency', dimensionsMap: { Source: 'uv', Outcome: 'success' }, statistic: 'p50', label: 'uv p50', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: warmerNamespace, metricName: 'FetchLatency', dimensionsMap: { Source: 'air_quality', Outcome: 'success' }, statistic: 'p50', label: 'air_quality p50', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: warmerNamespace, metricName: 'FetchLatency', dimensionsMap: { Source: 'tides', Outcome: 'success' }, statistic: 'p50', label: 'tides p50', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: warmerNamespace, metricName: 'FetchLatency', dimensionsMap: { Source: 'water_temperature', Outcome: 'success' }, statistic: 'p50', label: 'water_temp p50', period: defaultPeriod }),
                ],
                width: 12,
                period: defaultPeriod,
            }),
            new cloudwatch.GraphWidget({
                title: 'Fetch Count by Source & Outcome',
                left: [
                    new cloudwatch.Metric({ namespace: warmerNamespace, metricName: 'FetchCount', dimensionsMap: { Source: 'marine', Outcome: 'success' }, statistic: 'Sum', label: 'marine ✓', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: warmerNamespace, metricName: 'FetchCount', dimensionsMap: { Source: 'hrrr', Outcome: 'success' }, statistic: 'Sum', label: 'hrrr ✓', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: warmerNamespace, metricName: 'FetchCount', dimensionsMap: { Source: 'uv', Outcome: 'success' }, statistic: 'Sum', label: 'uv ✓', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: warmerNamespace, metricName: 'FetchCount', dimensionsMap: { Source: 'air_quality', Outcome: 'success' }, statistic: 'Sum', label: 'air_quality ✓', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: warmerNamespace, metricName: 'FetchCount', dimensionsMap: { Source: 'tides', Outcome: 'success' }, statistic: 'Sum', label: 'tides ✓', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: warmerNamespace, metricName: 'FetchCount', dimensionsMap: { Source: 'water_temperature', Outcome: 'success' }, statistic: 'Sum', label: 'water_temp ✓', period: defaultPeriod }),
                ],
                right: [
                    new cloudwatch.Metric({ namespace: warmerNamespace, metricName: 'FetchCount', dimensionsMap: { Source: 'marine', Outcome: 'error' }, statistic: 'Sum', label: 'marine ✗', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: warmerNamespace, metricName: 'FetchCount', dimensionsMap: { Source: 'hrrr', Outcome: 'error' }, statistic: 'Sum', label: 'hrrr ✗', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: warmerNamespace, metricName: 'FetchCount', dimensionsMap: { Source: 'tides', Outcome: 'error' }, statistic: 'Sum', label: 'tides ✗', period: defaultPeriod }),
                    new cloudwatch.Metric({ namespace: warmerNamespace, metricName: 'FetchCount', dimensionsMap: { Source: 'water_temperature', Outcome: 'error' }, statistic: 'Sum', label: 'water_temp ✗', period: defaultPeriod }),
                ],
                width: 12,
                period: defaultPeriod,
            }),
        );

        // --- Outputs ---

        new cdk.CfnOutput(this, 'ApiUrl', { value: api.url ?? 'undefined' });
        new cdk.CfnOutput(this, 'ApiKeyId', {
            value: apiKey.keyId,
            description: 'API Key ID — use `aws apigateway get-api-key --api-key <id> --include-value` to retrieve the key value',
        });
    }
}
