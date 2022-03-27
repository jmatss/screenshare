package se._2a.screenshare;

import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import com.google.gson.Gson;
import com.amazonaws.services.apigatewaymanagementapi.AmazonApiGatewayManagementApiClient;
import com.amazonaws.services.apigatewaymanagementapi.AmazonApiGatewayManagementApiClientBuilder;
import com.amazonaws.services.apigatewaymanagementapi.model.PostToConnectionRequest;
import com.amazonaws.services.dynamodbv2.model.AttributeValue;
import com.amazonaws.services.dynamodbv2.model.ComparisonOperator;
import com.amazonaws.services.dynamodbv2.model.Condition;
import com.amazonaws.services.dynamodbv2.model.ScanRequest;
import com.amazonaws.services.dynamodbv2.model.ScanResult;
import com.amazonaws.services.lambda.runtime.Context;
import com.amazonaws.services.lambda.runtime.LambdaLogger;
import com.amazonaws.services.lambda.runtime.RequestHandler;
import com.amazonaws.services.lambda.runtime.events.APIGatewayV2WebSocketEvent;
import com.amazonaws.services.lambda.runtime.events.APIGatewayV2WebSocketResponse;

public class CandidateHandler implements RequestHandler<APIGatewayV2WebSocketEvent, APIGatewayV2WebSocketResponse> {
    private DynamoDbConnection conn = new DynamoDbConnection();

    @Override
    public APIGatewayV2WebSocketResponse handleRequest(APIGatewayV2WebSocketEvent event, Context context) {
        LambdaLogger logger = context.getLogger();
        Gson gson = new Gson();

        String connectionId = event.getRequestContext().getConnectionId();

        String receiverConnectionId;
        String body;

        if (this.isConsumer(connectionId, logger)) {
            String producerConnectionId = this.getProducerConnectionId(logger);
            if (producerConnectionId == null) {
                String msg = "Unable to get connection ID for producer.";
                return ResponseUtil.createError(msg, logger);
            }

            Map<String, Object> bodyObj = (Map<String, Object>)gson.fromJson(event.getBody(), Map.class);
            bodyObj.put(Constants.ID, connectionId);
            body = gson.toJson(bodyObj);

            receiverConnectionId = producerConnectionId;
        } else {
            Map<String, Object> bodyObj = (Map<String, Object>)gson.fromJson(event.getBody(), Map.class);
            String consumerConnectionId = (String)bodyObj.remove(Constants.ID);
            body = gson.toJson(bodyObj);

            receiverConnectionId = consumerConnectionId;
        }

        PostToConnectionRequest post = new PostToConnectionRequest()
            .withConnectionId(receiverConnectionId)
            .withData(ByteBuffer.wrap(body.getBytes(StandardCharsets.UTF_8)));
        AmazonApiGatewayManagementApiClient.builder()
            .defaultClient()
            .postToConnection(post);

        return ResponseUtil.createSuccess("Candidate sent", logger);
    }

    private boolean isConsumer(String connectionId, LambdaLogger logger) {
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
        ScanResult scanResult = this.conn.get(logger).scan(scanRequest);

        return scanResult.getCount() > 0;
    }

    private String getProducerConnectionId(LambdaLogger logger) {
        String tableName = System.getenv(Constants.TABLE_NAME_ENV);
        String idColumnName = System.getenv(Constants.ID_COLUMN_NAME_ENV);
        String typeColumnName = System.getenv(Constants.TYPE_COLUMN_NAME_ENV);

        Map<String, Condition> scanFilter = new HashMap<>();
        Condition condition = new Condition()
            .withComparisonOperator(ComparisonOperator.EQ.toString())
            .withAttributeValueList(new AttributeValue(Constants.PRODUCER));
        scanFilter.put(typeColumnName, condition);

        ScanRequest scanRequest = new ScanRequest(tableName).withScanFilter(scanFilter);
        ScanResult scanResult = this.conn.get(logger).scan(scanRequest);

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
}
