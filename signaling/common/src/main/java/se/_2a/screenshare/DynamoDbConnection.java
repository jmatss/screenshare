package se._2a.screenshare;

import com.amazonaws.auth.AWSStaticCredentialsProvider;
import com.amazonaws.auth.BasicAWSCredentials;
import com.amazonaws.services.dynamodbv2.AmazonDynamoDB;
import com.amazonaws.services.dynamodbv2.AmazonDynamoDBClientBuilder;
import com.amazonaws.services.lambda.runtime.LambdaLogger;

/**
 * Wrapper around a DynamoDB connection to add lazy initialization and the
 * possiblity to reuse the connection multiple times for the handlers.
 */
public class DynamoDbConnection {
    private AmazonDynamoDB dynamoDbConnection;
    
    public DynamoDbConnection() {
    }

    public AmazonDynamoDB get(LambdaLogger logger) {
        if (this.dynamoDbConnection == null) {
            boolean missingCredentials = false;

            String id = System.getenv(Constants.DB_ACCESS_ID_ENV);
            if (id == null || id.isEmpty()) {
                String msg = "No DB Access ID set in env var " + Constants.DB_ACCESS_ID_ENV;
                logger.log(msg);
                missingCredentials = true;
            }

            String key = System.getenv(Constants.DB_ACCESS_KEY_ENV);
            if (key == null || key.isEmpty()) {
                String msg = "No DB Access key set in env var " + Constants.DB_ACCESS_KEY_ENV;
                logger.log(msg);
                missingCredentials = true;
            }

            if (!missingCredentials) {
                BasicAWSCredentials credentials = new BasicAWSCredentials(id, key);
                this.dynamoDbConnection = AmazonDynamoDBClientBuilder.standard()
                    .withCredentials(new AWSStaticCredentialsProvider(credentials))
                    .build();
            }
        }
        return this.dynamoDbConnection;
    }
}
