package se._2a.screenshare;

import java.util.HashMap;
import java.util.List;
import java.util.Map;

import com.amazonaws.services.dynamodbv2.AmazonDynamoDB;
import com.amazonaws.services.dynamodbv2.AmazonDynamoDBClientBuilder;
import com.amazonaws.services.dynamodbv2.model.AttributeValue;
import com.amazonaws.services.dynamodbv2.model.ComparisonOperator;
import com.amazonaws.services.dynamodbv2.model.Condition;
import com.amazonaws.services.dynamodbv2.model.ScanRequest;
import com.amazonaws.services.dynamodbv2.model.ScanResult;

/**
 * Wrapper around a DynamoDB connection to add lazy initialization and the
 * possiblity to reuse the connection multiple times for the handlers.
 */
public class DynamoDbConnection {
    private AmazonDynamoDB dynamoDbConnection;
    
    public DynamoDbConnection() {
    }

    public AmazonDynamoDB get() {
        if (this.dynamoDbConnection == null) {
            this.dynamoDbConnection = AmazonDynamoDBClientBuilder.defaultClient();
        }
        return this.dynamoDbConnection;
    }

    /**
     * Returns true if the given `connectionId` belongs to a consumer.
     * 
     * @param connectionId the connection ID to check.
     * @return true if the connection is a consumer, false if it is a producer.
     */
    public boolean isConsumer(String connectionId) {
        String tableName = System.getenv(Constants.TABLE_NAME_ENV);
        String idColumnName = System.getenv(Constants.ID_COLUMN_NAME_ENV);
        String typeColumnName = System.getenv(Constants.TYPE_COLUMN_NAME_ENV);

        Map<String, Condition> scanFilter = new HashMap<>();
        Condition conditionId = new Condition()
            .withComparisonOperator(ComparisonOperator.EQ.toString())
            .withAttributeValueList(new AttributeValue(connectionId));
        Condition conditionConsumer = new Condition()
            .withComparisonOperator(ComparisonOperator.EQ.toString())
            .withAttributeValueList(new AttributeValue(Constants.CONSUMER));
        scanFilter.put(idColumnName, conditionId);
        scanFilter.put(typeColumnName, conditionConsumer);

        ScanRequest scanRequest = new ScanRequest(tableName).withScanFilter(scanFilter);
        ScanResult scanResult = this.get().scan(scanRequest);

        return scanResult.getCount() > 0;
    }

    /**
     * Fetches the connection ID of the producer from the database. If no
     * producer is currently connected, null is returned.
     * 
     * @return the connection ID of the connected producer or null.
     */
    public String getProducerConnectionId() {
        String tableName = System.getenv(Constants.TABLE_NAME_ENV);
        String idColumnName = System.getenv(Constants.ID_COLUMN_NAME_ENV);
        String typeColumnName = System.getenv(Constants.TYPE_COLUMN_NAME_ENV);

        Map<String, Condition> scanFilter = new HashMap<>();
        Condition condition = new Condition()
            .withComparisonOperator(ComparisonOperator.EQ.toString())
            .withAttributeValueList(new AttributeValue(Constants.PRODUCER));
        scanFilter.put(typeColumnName, condition);

        ScanRequest scanRequest = new ScanRequest(tableName).withScanFilter(scanFilter);
        ScanResult scanResult = this.get().scan(scanRequest);

        List<Map<String, AttributeValue>> items = scanResult.getItems();
        if (items != null) {
            for (Map<String, AttributeValue> item : items) {
                AttributeValue connectionId = item.get(idColumnName);
                if (connectionId != null) {
                    return connectionId.getS();
                }
            }
        }
        return null;
    }

    /**
     * Returns true if a producer can be found in the database.
     */
    public boolean isProducerConnected() {
        return this.getProducerConnectionId() != null;
    }
}
