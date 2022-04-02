package se._2a.screenshare;

import java.lang.reflect.Type;
import java.util.Map;

import com.google.gson.Gson;
import com.google.gson.reflect.TypeToken;
import com.amazonaws.services.lambda.runtime.Context;
import com.amazonaws.services.lambda.runtime.LambdaLogger;
import com.amazonaws.services.lambda.runtime.RequestHandler;
import com.amazonaws.services.lambda.runtime.events.APIGatewayV2WebSocketEvent;
import com.amazonaws.services.lambda.runtime.events.APIGatewayV2WebSocketEvent.RequestContext;
import com.amazonaws.services.lambda.runtime.events.APIGatewayV2WebSocketResponse;

/**
 * Class used to "proxy" WebRTC Candidate messages between two connected websockets.
 */
public class CandidateHandler implements RequestHandler<APIGatewayV2WebSocketEvent, APIGatewayV2WebSocketResponse> {
    public static final String HANDLER_TYPE = Constants.CANDIDATE;

    private DynamoDbConnection conn = new DynamoDbConnection();

    @Override
    public APIGatewayV2WebSocketResponse handleRequest(APIGatewayV2WebSocketEvent event, Context context) {
        LambdaLogger logger = context.getLogger();
        Gson gson = new Gson();
        Type jsonType = new TypeToken<Map<String, Object>>(){}.getType();

        RequestContext requestContext = event.getRequestContext();
        String connectionId = requestContext.getConnectionId();
        String receiverConnectionId;
        String body;

        if (this.conn.isConsumer(connectionId)) {
            String producerConnectionId = this.conn.getProducerConnectionId();
            if (producerConnectionId == null) {
                String msg = "Unable to get connection ID for producer.";
                return ResponseUtil.createError(msg, HANDLER_TYPE, logger);
            }

            Map<String, Object> bodyObj = gson.fromJson(event.getBody(), jsonType);
            bodyObj.put(Constants.ID, connectionId);
            body = gson.toJson(bodyObj);

            receiverConnectionId = producerConnectionId;
        } else {
            Map<String, Object> bodyObj = gson.fromJson(event.getBody(), jsonType);
            String consumerConnectionId = (String)bodyObj.remove(Constants.ID);
            body = gson.toJson(bodyObj);

            receiverConnectionId = consumerConnectionId;
        }

        ResponseUtil.pushToClient(receiverConnectionId, body, requestContext, logger);
        return ResponseUtil.createSuccess("Candidate sent", HANDLER_TYPE, logger);
    }
}
