package se._2a.screenshare;

import java.util.Collections;
import java.util.HashMap;
import java.util.Map;

import com.amazonaws.services.dynamodbv2.AmazonDynamoDB;
import com.amazonaws.services.dynamodbv2.AmazonDynamoDBClientBuilder;
import com.amazonaws.services.dynamodbv2.model.AttributeValue;
import com.amazonaws.services.lambda.runtime.Context;
import com.amazonaws.services.lambda.runtime.LambdaLogger;
import com.amazonaws.services.lambda.runtime.RequestHandler;
import com.amazonaws.services.lambda.runtime.events.APIGatewayV2WebSocketEvent;
import com.amazonaws.services.lambda.runtime.events.APIGatewayV2WebSocketResponse;

public class DisconnectHandler implements RequestHandler<APIGatewayV2WebSocketEvent, APIGatewayV2WebSocketResponse> {
    private DynamoDbConnection conn = new DynamoDbConnection();

    @Override
    public APIGatewayV2WebSocketResponse handleRequest(APIGatewayV2WebSocketEvent event, Context context) {
        LambdaLogger logger = context.getLogger();

        String connectionId = event.getRequestContext().getConnectionId();
        String tableName = System.getenv(Constants.TABLE_NAME_ENV);
        String idColumnName = System.getenv(Constants.ID_COLUMN_NAME_ENV);

        Map<String, AttributeValue> columnProperties = new HashMap<>();
        columnProperties.put(idColumnName, new AttributeValue(connectionId));
        this.conn.get(logger).deleteItem(tableName, columnProperties);

        return ResponseUtil.createSuccess("Disconnected", logger);
    }
}
