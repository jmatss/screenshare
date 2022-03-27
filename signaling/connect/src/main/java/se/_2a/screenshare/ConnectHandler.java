package se._2a.screenshare;

import java.util.HashMap;
import java.util.Map;

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

public class ConnectHandler implements RequestHandler<APIGatewayV2WebSocketEvent, APIGatewayV2WebSocketResponse> {
    private DynamoDbConnection conn = new DynamoDbConnection();

    @Override
    public APIGatewayV2WebSocketResponse handleRequest(APIGatewayV2WebSocketEvent event, Context context) {
        LambdaLogger logger = context.getLogger();

        String connectionId = event.getRequestContext().getConnectionId();
        String tableName = System.getenv(Constants.TABLE_NAME_ENV);
        String idColumnName = System.getenv(Constants.ID_COLUMN_NAME_ENV);
        String typeColumnName = System.getenv(Constants.TYPE_COLUMN_NAME_ENV);

        // An empty or missing `type` defaults to the value `consumer`.
        Map<String, String> params = event.getQueryStringParameters();
        String connectType = (params != null) ? params.get(typeColumnName) : null;
        if (connectType == null || connectType.isEmpty()) {
            connectType = Constants.CONSUMER;
        }

        logger.log("handleRequest -- id: " + connectionId + ", type: " + connectType);

        switch (connectType) {
            case Constants.CONSUMER:
                if (!this.isProducerConnected(logger)) {
                    String msg = "Unable to find a connected producer.";
                    msg += " A producer must be connected before consumers can connect.";
                    return ResponseUtil.createError(msg, logger);
                }
                break;

            case Constants.PRODUCER:
                if (this.isProducerConnected(logger)) {
                    String msg = "A producer is already connected.";
                    msg += " There can be only one producer at a time.";
                    return ResponseUtil.createError(msg, logger);
                }
                break;

            default:
                String msg = "Invalid type set in connect message: " + connectType;
                return ResponseUtil.createError(msg, logger);
        }

        Map<String, AttributeValue> columnProperties = new HashMap<>();
        columnProperties.put(idColumnName, new AttributeValue(connectionId));
        columnProperties.put(typeColumnName, new AttributeValue(connectType));
        this.conn.get(logger).putItem(tableName, columnProperties);

        return ResponseUtil.createSuccess("Connected", logger);
    }

    private boolean isProducerConnected(LambdaLogger logger) {
        String tableName = System.getenv(Constants.TABLE_NAME_ENV);
        String typeColumnName = System.getenv(Constants.TYPE_COLUMN_NAME_ENV);

        Map<String, Condition> scanFilter = new HashMap<>();
        Condition condition = new Condition()
            .withComparisonOperator(ComparisonOperator.EQ)
            .withAttributeValueList(new AttributeValue(Constants.PRODUCER));
        scanFilter.put(typeColumnName, condition);

        ScanRequest scanRequest = new ScanRequest(tableName).withScanFilter(scanFilter);
        ScanResult scanResult = this.conn.get(logger).scan(scanRequest);

        return scanResult.getCount() > 0;
    }
}
