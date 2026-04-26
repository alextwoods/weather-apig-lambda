#!/usr/bin/env node
import * as cdk from 'aws-cdk-lib';
import { WeatherApigLambdaStack } from '../lib/weather-apig-lambda-stack';

const app = new cdk.App();
new WeatherApigLambdaStack(app, 'WeatherApigLambdaStack', {
    env: {
        region: 'us-west-1',
        account: '655347895545',
    },
});
